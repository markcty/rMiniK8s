use std::{
    cmp::{min, Ordering},
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use anyhow::{anyhow, Context, Error, Result};
use resources::{
    informer::Store,
    models::Response,
    objects::{
        gpu_job::{GpuJob, GpuJobStatus},
        object_reference::ObjectReference,
        pod::{Pod, PodPhase},
        KubeObject, Object,
    },
};
use tokio::{
    select,
    sync::{mpsc, mpsc::Receiver},
    task::JoinHandle,
};

use crate::{
    utils::{create_informer, get_job_filename, Event, ResyncNotification},
    BASE_IMG, CONFIG, GPU_SERVER_CONFIG, TMP_DIR,
};

pub struct GpuJobController {
    job_rx: Receiver<Event<GpuJob>>,
    job_resync_rx: Receiver<ResyncNotification>,
    job_informer: Option<JoinHandle<Result<(), Error>>>,
    job_store: Store<GpuJob>,

    pod_rx: Receiver<Event<Pod>>,
    pod_resync_rx: Receiver<ResyncNotification>,
    pod_informer: Option<JoinHandle<Result<(), Error>>>,
    pod_store: Store<Pod>,
}

impl GpuJobController {
    pub fn new() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Event<GpuJob>>(16);
        let (job_resync_tx, job_resync_rx) = mpsc::channel::<ResyncNotification>(16);
        let job_informer = create_informer::<GpuJob>("gpujobs".to_string(), job_tx, job_resync_tx);
        let job_store = job_informer.get_store();

        let (pod_tx, pod_rx) = mpsc::channel::<Event<Pod>>(16);
        let (pod_resync_tx, pod_resync_rx) = mpsc::channel::<ResyncNotification>(16);
        let pod_informer = create_informer::<Pod>("pods".to_string(), pod_tx, pod_resync_tx);
        let pod_store = pod_informer.get_store();

        let job_informer = tokio::spawn(async move { job_informer.run().await });
        let pod_informer = tokio::spawn(async move { pod_informer.run().await });

        Self {
            job_rx,
            job_resync_rx,
            job_informer: Some(job_informer),
            job_store,

            pod_rx,
            pod_resync_rx,
            pod_informer: Some(pod_informer),
            pod_store,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("GpuJob Controller started");

        loop {
            select! {
                Some(event) = self.job_rx.recv() => {
                    let result = match event {
                        Event::Add(mut job) => {
                            self.create_pod_template(&mut job).await?;
                            self.reconcile(job).await
                        }
                        Event::Update(_, job) => self.reconcile(job).await,
                        Event::Delete(mut job) => {
                                // FIXME: Remove all pods instead of one
                                job.spec.parallelism = 0;
                                self.reconcile(job).await
                        },
                    };
                    if let Err(e) = result {
                        tracing::error!("Error while processing GpuJob update: {:#}", e);
                    }
                },
                Some(event) = self.pod_rx.recv() => {
                    let result = match event {
                        Event::Add(pod) | Event::Update(_, pod) | Event::Delete(pod) => self.handle_pod_change(pod).await
                    };
                    if let Err(e) = result {
                        tracing::error!("Error while processing Pod update: {:#}", e);
                    }
                },
                _ = self.job_resync_rx.recv() => self.resync_job().await,
                _ = self.pod_resync_rx.recv() => self.resync_pod().await,
                else => break
            }
        }

        let job_informer = std::mem::replace(&mut self.job_informer, None);
        job_informer.unwrap().await??;
        let pod_informer = std::mem::replace(&mut self.pod_informer, None);
        pod_informer.unwrap().await??;
        tracing::info!("GpuJob Controller exited");
        Ok(())
    }

    async fn handle_pod_change(&self, pod: Pod) -> Result<()> {
        let name = pod.metadata.name.to_owned();
        let owner = self.resolve_owner(&pod.metadata.owner_references).await;
        match owner {
            Some(owner) => {
                self.update_gpujob(owner).await?;
            },
            None => tracing::debug!("Pod {} is not owned by any GpuJob", name),
        };
        Ok(())
    }

    async fn update_gpujob(&self, mut job: GpuJob) -> Result<()> {
        let name = job.metadata.name.to_owned();
        let new_status = self.calculate_status(&job).await;

        let status = job
            .status
            .as_mut()
            .with_context(|| format!("GpuJob {} has no status", name))?;

        if !new_status.eq(status) {
            tracing::info!(
                "Observed change in GpuJob {}, failed: {} -> {}, active: {} -> {}, succeeded: {} -> {}",
                name,
                status.failed,
                new_status.failed,
                status.active,
                new_status.active,
                status.succeeded,
                new_status.succeeded
            );
            job.status = Some(new_status);
            self.post_status(job).await?;
        }
        Ok(())
    }

    async fn create_pod_template(&self, job: &mut GpuJob) -> Result<()> {
        self.prepare_file(job).await?;
        self.build_image(job).await?;
        Ok(())
    }

    async fn prepare_file(&self, job: &GpuJob) -> Result<()> {
        self.fetch_code(job).await?;
        self.gen_config_file(job).await?;
        self.gen_docker_file(job).await?;
        Ok(())
    }

    async fn fetch_code(&self, job: &GpuJob) -> Result<()> {
        let client = reqwest::Client::new();
        let filename = get_job_filename(job)?;
        let response = client
            .get(format!("{}/api/v1/tmp/{}", CONFIG.api_server_url, filename))
            .send()
            .await?;

        if let Err(err) = response.error_for_status_ref() {
            return Err(anyhow!(
                "Error get file {}. status: {}",
                filename,
                err.status().unwrap_or_default()
            ));
        }

        let job_dir_path = std::path::Path::new(TMP_DIR).join(PathBuf::from(job.name()));
        fs::create_dir_all(&job_dir_path)?;
        let code_path = job_dir_path.join(PathBuf::from(filename));
        let mut file = File::create(code_path)?;
        let content = response.bytes().await?;
        file.write_all(content.as_ref())?;

        Ok(())
    }

    async fn gen_config_file(&self, job: &GpuJob) -> Result<()> {
        let job_dir_path = std::path::Path::new(TMP_DIR).join(PathBuf::from(job.name()));
        fs::create_dir_all(&job_dir_path)?;
        let config_path = job_dir_path.join(PathBuf::from(format!("{}.slurm", job.name())));
        let mut file = File::create(config_path)?;
        let slurm_config = job.spec.gpu_config.slurm_config.to_owned();

        let mut content = "#!/bin/bash\n\n".to_string();
        content.push_str(format!("#SBATCH --job-name={}\n", job.name()).as_str());
        content.push_str(format!("#SBATCH --partition={}\n", slurm_config.partition).as_str());
        content.push_str("#SBATCH --output=%j.out\n");
        content.push_str("#SBATCH --error=%j.err\n");
        content.push_str(format!("#SBATCH -N {}\n", slurm_config.total_core_number).as_str());
        content.push_str(
            format!(
                "#SBATCH --ntasks-per-node={}\n",
                slurm_config.ntasks_per_node
            )
            .as_str(),
        );
        content
            .push_str(format!("#SBATCH --cpus-per-task={}\n", slurm_config.cpus_per_task).as_str());
        content.push_str(format!("#SBATCH --gres={}\n", slurm_config.gres).as_str());
        if let Some(scripts) = slurm_config.scripts {
            for script in scripts {
                content.push_str(format!("{}\n", script).as_str());
            }
        }

        file.write_all(content.as_ref())?;

        Ok(())
    }

    async fn gen_docker_file(&self, job: &GpuJob) -> Result<()> {
        let code_file_name = get_job_filename(job)?;
        let job_dir_path = std::path::Path::new(TMP_DIR).join(PathBuf::from(job.name()));
        let docker_file_path = job_dir_path.join("Dockerfile");
        let mut docker_file = File::create(docker_file_path)?;
        let mut content = format!("FROM {}\n", BASE_IMG);
        content.push_str(format!("COPY . {}\n", "/code/").as_str());
        content.push_str(
            format!(
                "ENV USERNAME={} PASSWORD={} JOB_NAME={}\n",
                GPU_SERVER_CONFIG.username,
                GPU_SERVER_CONFIG.password,
                job.name()
            )
            .as_str(),
        );
        content.push_str(
            format!(
                "ENV CODE_FILE_NAME={} COMPILE_SCRIPT={}\n",
                code_file_name, job.spec.gpu_config.compile_scripts
            )
            .as_str(),
        );

        docker_file.write_all(content.as_ref())?;
        Ok(())
    }

    async fn build_image(&self, job: &GpuJob) -> Result<()> {
        Ok(())
    }

    async fn reconcile(&self, job: GpuJob) -> Result<()> {
        let status = job
            .status
            .as_ref()
            .with_context(|| "GpuJob has no status")?;
        tracing::info!("Tuning GpuJob {}", job.metadata.name);

        let want_active = min(
            job.spec.completions - status.succeeded,
            job.spec.parallelism,
        );
        match status.active.cmp(&want_active) {
            Ordering::Less => {
                // Create a new pod, if more pods are needed, they'll be created later
                self.create_pod(&job).await?;
            },
            Ordering::Greater => {
                // Delete active pods
                let pod_name = self.get_pod_to_delete(&job).await;
                self.delete_pod(pod_name).await?;
            },
            Ordering::Equal => {
                // Nothing to do
            },
        }
        Ok(())
    }

    async fn post_status(&self, job: GpuJob) -> Result<()> {
        let client = reqwest::Client::new();
        let name = job.metadata.name.to_owned();
        let response = client
            .put(format!("{}{}", CONFIG.api_server_url, job.uri()))
            .json(&KubeObject::GpuJob(job))
            .send()
            .await?
            .json::<Response<()>>()
            .await
            .with_context(|| "Error posting status")?;
        tracing::info!(
            "Posted status for GpuJob {}: {}",
            name,
            response.msg.unwrap_or_else(|| "".to_string())
        );
        Ok(())
    }

    async fn create_pod(&self, job: &GpuJob) -> Result<()> {
        let client = reqwest::Client::new();
        let job_status = job
            .status
            .as_ref()
            .with_context(|| "GpuJob has no status")?;
        let template = job_status
            .template
            .as_ref()
            .with_context(|| "GpuJob has no pod template")?;
        let mut metadata = template.metadata.clone();
        metadata.owner_references.push(job.object_reference());
        let pod = Pod {
            metadata,
            spec: template.spec.clone(),
            status: None,
        };
        let response = client
            .post(format!("{}/api/v1/pods", CONFIG.api_server_url))
            .json(&KubeObject::Pod(pod))
            .send()
            .await?
            .json::<Response<()>>()
            .await
            .with_context(|| "Error creating pod")?;
        if let Some(msg) = response.msg {
            tracing::info!("{}", msg);
        }
        Ok(())
    }

    async fn delete_pod(&self, name: String) -> Result<()> {
        let client = reqwest::Client::new();
        let response = client
            .delete(format!("{}/api/v1/pods/{}", CONFIG.api_server_url, name))
            .send()
            .await?
            .json::<Response<()>>()
            .await
            .with_context(|| "Error deleting pod")?;
        if let Some(msg) = response.msg {
            tracing::info!("{}", msg);
        }
        Ok(())
    }

    async fn resolve_owner(&self, refs: &[ObjectReference]) -> Option<GpuJob> {
        let owners: Vec<&ObjectReference> = refs
            .iter()
            .filter(|r| r.kind == "GpuJob")
            .collect::<Vec<_>>();
        if owners.len() != 1 {
            tracing::debug!("Pod doesn't have exactly one owner");
            return None;
        }
        // Clone the object and drop the reference,
        // otherwise informer may deadlock when handling watch event
        let store = self.job_store.read().await;
        let res = store.get(format!("/api/v1/gpujobs/{}", owners[0].name).as_str());
        res.cloned()
    }

    async fn get_pods(&self, job: &GpuJob) -> Vec<Pod> {
        let store = self.pod_store.read().await;
        store
            .iter()
            .filter(|(_, pod)| {
                pod.metadata
                    .owner_references
                    .contains(&job.object_reference())
            })
            .map(|(_, pod)| pod.to_owned())
            .collect::<Vec<_>>()
    }

    async fn get_active_pods(&self, job: &GpuJob) -> Vec<Pod> {
        let store = self.pod_store.read().await;
        store
            .iter()
            .filter(|(_, pod)| {
                pod.is_active()
                    && pod
                        .metadata
                        .owner_references
                        .contains(&job.object_reference())
            })
            .map(|(_, pod)| pod.to_owned())
            .collect::<Vec<_>>()
    }

    async fn calculate_status(&self, job: &GpuJob) -> GpuJobStatus {
        let mut active: u32 = 0;
        let mut failed: u32 = 0;
        let mut succeeded: u32 = 0;
        let pods = self.get_pods(job).await;

        pods.iter().for_each(|pod| {
            if let Some(status) = &pod.status {
                match status.phase {
                    PodPhase::Failed => failed += 1,
                    PodPhase::Succeeded => succeeded += 1,
                    _ => active += 1,
                }
            } else {
                tracing::error!("Pod {} should have status", pod.name())
            }
        });

        GpuJobStatus {
            active,
            failed,
            succeeded,
            ..job
                .status
                .to_owned()
                .unwrap_or_else(|| panic!("job {} should have status", job.name()))
        }
    }

    async fn get_pod_to_delete(&self, job: &GpuJob) -> String {
        let mut pods = self.get_active_pods(job).await;
        pods.sort_by(|a, b| {
            let status_a = a.status.as_ref().unwrap();
            let status_b = b.status.as_ref().unwrap();
            if status_a.phase != status_b.phase {
                let order = HashMap::from([
                    (PodPhase::Failed, 0),
                    (PodPhase::Pending, 1),
                    (PodPhase::Running, 2),
                    (PodPhase::Succeeded, 3),
                ]);
                return order
                    .get(&status_a.phase)
                    .unwrap()
                    .cmp(order.get(&status_b.phase).unwrap());
            }
            // Not Ready < Ready
            if a.is_ready() != b.is_ready() {
                return a.is_ready().cmp(&b.is_ready());
            }
            if status_a.start_time != status_b.start_time {
                return status_a.start_time.cmp(&status_b.start_time);
            }
            Ordering::Equal
        });
        pods[0].metadata.name.to_owned()
    }

    async fn resync_job(&self) {
        let store = self.job_store.read().await;
        for job in store.values() {
            let result = self.update_gpujob(job.to_owned()).await;
            if let Err(e) = result {
                tracing::error!("Failed to update GpuJob {}: {:#?}", job.metadata.name, e);
            }
        }
    }

    async fn resync_pod(&self) {
        let mut job_to_resync = HashSet::<String>::new();
        let store = self.pod_store.read().await;
        for pod in store.values() {
            let owner = self.resolve_owner(&pod.metadata.owner_references).await;
            match owner {
                Some(owner) => {
                    let job_name = owner.metadata.name.to_owned();
                    if !job_to_resync.contains(&job_name) {
                        job_to_resync.insert(job_name.to_owned());
                        let result = self.update_gpujob(owner).await;
                        if let Err(e) = result {
                            tracing::error!("Failed to update GpuJob {}: {:#?}", job_name, e);
                        }
                    }
                },
                None => tracing::debug!("Pod {} is not owned by any GpuJob", pod.metadata.name),
            };
        }
    }
}
