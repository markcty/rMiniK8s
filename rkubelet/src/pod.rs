use std::{
    collections::HashMap, default::Default, fs::create_dir_all, net::Ipv4Addr, path::PathBuf,
};

use anyhow::{Context, Result};
use bollard::{
    container::Config,
    models::{ContainerInspectResponse, HostConfig},
};
use futures::future::join_all;
use resources::{
    objects::{
        pod,
        pod::{
            ContainerState, ContainerStatus, ImagePullPolicy, PodCondition, PodConditionType,
            PodPhase, PodSpec, PodStatus, RestartPolicy, VolumeMount,
        },
        KubeObject, Metadata,
    },
    utils::first_error_or_ok,
};
use uuid::Uuid;

use crate::{
    config::{CONTAINER_NAME_PREFIX, PAUSE_IMAGE_NAME, POD_DIR_PATH, SANDBOX_NAME},
    docker,
    docker::{Container, Image},
    volume::Volume,
};

#[derive(Debug)]
struct PodActions {
    pub create_sandbox: bool,
    pub start_sandbox: bool,
    pub containers_to_start: Vec<pod::Container>,
    pub containers_to_remove: Vec<Container>,
}

#[derive(Debug)]
pub struct Pod {
    metadata: Metadata,
    spec: PodSpec,
    status: PodStatus,
}

impl Pod {
    pub fn load(pod: pod::Pod) -> Result<Self> {
        let status = pod
            .status
            .with_context(|| "[Pod::load] Status is missing")?;
        Ok(Pod {
            metadata: pod.metadata,
            spec: pod.spec,
            status,
        })
    }

    pub async fn create(pod: pod::Pod) -> Result<Self> {
        let name = pod.metadata.name.to_owned();
        tracing::info!("Creating pod {}...", name);
        let mut pod = Self {
            metadata: pod.metadata,
            spec: pod.spec,
            status: PodStatus::default(),
        };
        create_dir_all(pod.dir())
            .with_context(|| format!("Failed to create pod directory for {}", name))?;

        let sandbox = pod.create_sandbox().await?;
        pod.create_containers(&sandbox).await?;
        pod.update_status().await?;

        Ok(pod)
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting pod {}...", self.metadata.name);
        let containers = self
            .status
            .container_statuses
            .iter()
            .map(Container::from)
            .collect::<Vec<Container>>();
        let results = docker::start_containers(&containers).await;
        first_error_or_ok(results).map(|_| tracing::info!("Pod {} created", self.metadata.name))
    }

    pub async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping pod {}...", self.metadata.name);
        self.stop_containers()
            .await
            .map(|_| tracing::info!("Pod {} stopped", self.metadata.name))
    }

    pub async fn remove(&self) -> Result<()> {
        tracing::info!("Removing pod {}...", self.metadata.name);
        if let Err(err) = self.stop().await {
            tracing::error!("Failed to stop pod: {:#}", err);
        }
        self.remove_containers()
            .await
            .map(|_| tracing::info!("Pod {} removed", self.metadata.name))
        // TODO: Remove pod directory (DANGEROUS!)
    }

    /// Create a pod container
    async fn create_container(
        &self,
        container: &pod::Container,
        sandbox: &Container,
    ) -> Result<Container> {
        let image = Image::new(&container.image);
        image
            .pull_with_policy(container.image_pull_policy())
            .await?;
        let mode = Some(format!("container:{}", sandbox.id()));

        let volumes = self.create_volumes()?;
        let binds = self.bind_mounts(&volumes, &container.volume_mounts)?;

        let host_config = Some(HostConfig {
            cpu_shares: Some(container.resources.cpu_shares()),
            memory: Some(container.resources.limits.memory),
            network_mode: mode.to_owned(),
            ipc_mode: mode.to_owned(),
            pid_mode: mode.to_owned(),
            binds,
            restart_policy: Some((&self.spec.restart_policy).into()),
            ..Default::default()
        });
        let mut labels = self.container_labels();
        labels.insert(
            format!("{}.container.name", CONTAINER_NAME_PREFIX),
            container.name.to_owned(),
        );
        labels.insert(
            format!("{}.container.type", CONTAINER_NAME_PREFIX),
            "container".to_string(),
        );
        let config = Config {
            image: Some(image.name().to_owned()),
            entrypoint: Some(container.command.to_owned()),
            host_config,
            labels: Some(labels),
            ..Default::default()
        };
        let name = Some(self.get_container_name(&container.name));
        Container::create(name, config).await
    }

    /// Create pod containers
    async fn create_containers(&self, sandbox: &Container) -> Result<Vec<Container>> {
        let tasks = self
            .spec
            .containers
            .iter()
            .map(|c| self.create_container(c, sandbox))
            .collect::<Vec<_>>();
        let results = join_all(tasks).await;
        results.iter().for_each(|r| {
            if let Err(e) = r {
                tracing::error!("Failed to create pod container: {:#}", e);
            }
        });
        Ok(results.into_iter().filter_map(|r| r.ok()).collect())
    }

    async fn create_sandbox(&self) -> Result<Container> {
        let image = Image::new(PAUSE_IMAGE_NAME);
        image
            .pull_with_policy(ImagePullPolicy::IfNotPresent)
            .await?;
        let host_config = Some(HostConfig {
            network_mode: Some(self.spec.network_mode()),
            ipc_mode: Some("shareable".to_string()),
            ..Default::default()
        });
        let mut labels = self.container_labels();
        labels.insert(
            format!("{}.container.name", CONTAINER_NAME_PREFIX),
            SANDBOX_NAME.to_owned(),
        );
        labels.insert(
            format!("{}.container.type", CONTAINER_NAME_PREFIX),
            "podsandbox".to_string(),
        );
        let config = Config {
            image: Some(image.name().to_owned()),
            host_config,
            labels: Some(labels),
            exposed_ports: Some(self.spec.exposed_ports()),
            ..Default::default()
        };
        let name = Some(self.get_container_name(SANDBOX_NAME));
        let container = Container::create(name, config).await?;
        container.start().await?;
        Ok(container)
    }

    // Start a pod container, create it if doesn't exist
    async fn start_container(&self, container: &pod::Container, sandbox: &Container) -> Result<()> {
        let old_container = Container::new(self.get_container_name(&container.name));
        let result = old_container.inspect().await?;
        let container = match result {
            Some(_) => old_container,
            None => self.create_container(container, sandbox).await?,
        };
        container.start().await
    }

    /// Start pod containers, create them if doesn't exist
    async fn start_containers(
        &self,
        containers: &[pod::Container],
        sandbox: &Container,
    ) -> Result<()> {
        let tasks = containers
            .iter()
            .map(|c| self.start_container(c, sandbox))
            .collect::<Vec<_>>();
        let results = join_all(tasks).await;
        results.iter().for_each(|r| {
            if let Err(e) = r {
                tracing::error!("Failed to start container: {:#}", e);
            }
        });
        first_error_or_ok(results)
    }

    /// Inspect all pod containers
    async fn inspect_containers(&self) -> Result<Vec<Option<ContainerInspectResponse>>> {
        let containers = self.containers();
        let tasks = containers.iter().map(|c| c.inspect()).collect::<Vec<_>>();
        let results = join_all(tasks).await;
        results.iter().for_each(|r| {
            if let Err(e) = r {
                tracing::error!("Failed to inspect container: {:#}", e);
            }
        });
        Ok(results.into_iter().filter_map(|r| r.ok()).collect())
    }

    /// Stop all pod containers
    async fn stop_containers(&self) -> Result<()> {
        let containers = self.containers();
        let results = docker::stop_containers(&containers).await;
        self.sandbox().stop().await?;
        first_error_or_ok(results)
    }

    /// Remove all pod containers
    async fn remove_containers(&self) -> Result<()> {
        let containers = self.containers();
        let results = docker::remove_containers(&containers).await;
        self.sandbox().remove().await?;
        first_error_or_ok(results)
    }

    fn create_volumes(&self) -> Result<HashMap<String, Volume>> {
        let mut volumes = HashMap::new();
        for volume in &self.spec.volumes {
            let v = Volume::create(self.dir(), volume.to_owned())?;
            volumes.insert(volume.name.to_owned(), v);
        }
        Ok(volumes)
    }

    /// Update pod status, return true if pod status changed.
    pub async fn update_status(&mut self) -> Result<bool> {
        let response = self.sandbox().inspect().await?;
        let pod_ip = response
            .and_then(|r| Pod::get_ip(&r).map(|ip| ip.parse::<Ipv4Addr>().ok()))
            .flatten();
        let containers = self.inspect_containers().await?;
        // Missing containers will be filtered out
        let container_statuses = containers
            .into_iter()
            .flatten()
            .map(ContainerStatus::from)
            .collect::<Vec<_>>();
        let phase = self.compute_phase(&container_statuses);
        // TODO: determine pod conditions
        let new_status = PodStatus {
            phase,
            pod_ip,
            conditions: self.determine_conditions(&container_statuses),
            container_statuses,
            ..self.status.clone()
        };
        let changed = new_status != self.status;
        self.status = new_status;
        Ok(changed)
    }

    fn get_ip(response: &ContainerInspectResponse) -> Option<&String> {
        response
            .network_settings
            .as_ref()?
            .networks
            .as_ref()?
            .get("bridge")?
            .ip_address
            .as_ref()
    }

    fn bind_mounts(
        &self,
        volumes: &HashMap<String, Volume>,
        volume_mounts: &Vec<VolumeMount>,
    ) -> Result<Option<Vec<String>>> {
        if volume_mounts.is_empty() {
            return Ok(None);
        }
        let mut binds = vec![];
        for volume_mount in volume_mounts {
            let volume = volumes
                .get(&volume_mount.name)
                .with_context(|| format!("No volume {} found to mount", volume_mount.name))?;
            let host_src = volume.host_src();
            let container_dest = volume_mount.mount_path.to_owned();

            binds.push(format!("{}:{}", host_src, container_dest.to_owned()));
        }
        Ok(Some(binds))
    }

    fn get_container_name(&self, container_name: &str) -> String {
        let uid = self.metadata.uid.expect("Pod with no uid");
        Pod::unique_container_name(&uid, &self.metadata.name, container_name)
    }

    pub fn unique_container_name(uid: &Uuid, pod_name: &str, container_name: &str) -> String {
        format!(
            "{}_{}_{}_{}",
            CONTAINER_NAME_PREFIX, container_name, pod_name, uid
        )
    }

    fn sandbox(&self) -> Container {
        Container::new(self.get_container_name(SANDBOX_NAME))
    }

    fn containers(&self) -> Vec<Container> {
        self.spec
            .containers
            .iter()
            .map(|container| Container::new(self.get_container_name(&container.name)))
            .collect()
    }

    fn dir(&self) -> PathBuf {
        let uid = self.metadata.uid.expect("Pod with no uid");
        let mut dir = PathBuf::from(POD_DIR_PATH);
        dir.push(uid.to_string());
        dir
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn object(&self) -> KubeObject {
        KubeObject::Pod(pod::Pod {
            metadata: self.metadata.clone(),
            spec: self.spec.clone(),
            status: Some(self.status.clone()),
        })
    }

    fn compute_phase(&self, containers: &Vec<ContainerStatus>) -> PodPhase {
        let mut running = 0;
        let mut waiting = 0;
        let mut stopped = 0;
        let mut succeeded = 0;
        for container in containers {
            match container.state {
                ContainerState::Running => running += 1,
                ContainerState::Terminated {
                    exit_code,
                } => {
                    stopped += 1;
                    if exit_code == 0 {
                        succeeded += 1;
                    }
                },
                ContainerState::Waiting => waiting += 1,
            }
        }

        if waiting > 0 {
            return PodPhase::Pending;
        }
        if running > 0 {
            return PodPhase::Running;
        }
        if running == 0 && stopped > 0 {
            if self.spec.restart_policy == RestartPolicy::Always {
                // Restarting
                return PodPhase::Running;
            }
            if stopped == succeeded {
                return PodPhase::Succeeded;
            }
            if self.spec.restart_policy == RestartPolicy::Never {
                // If stopped != succeeded, at least one container failed
                return PodPhase::Failed;
            }
            return PodPhase::Running;
        }
        PodPhase::Pending
    }

    fn determine_conditions(
        &self,
        containers: &[ContainerStatus],
    ) -> HashMap<PodConditionType, PodCondition> {
        let mut conditions = self.status.conditions.to_owned();
        // Make sure running container count equals that in spec,
        // in case no pod container remains
        let ready_count = containers
            .iter()
            .map(|status| (status.state == ContainerState::Running) as u32)
            .sum::<u32>();
        let containers_ready = ready_count == self.spec.containers.len() as u32;
        conditions.insert(
            PodConditionType::ContainersReady,
            PodCondition {
                status: containers_ready,
            },
        );
        // TODO: Since we don't have readiness gates, this is equivalent to ContainersReady
        conditions.insert(
            PodConditionType::Ready,
            PodCondition {
                status: containers_ready,
            },
        );
        conditions
    }

    fn container_labels(&self) -> HashMap<String, String> {
        let mut labels = self.metadata.labels.0.to_owned();
        labels.insert(
            format!("{}.pod.name", CONTAINER_NAME_PREFIX),
            self.metadata.name.to_owned(),
        );
        labels.insert(
            format!("{}.pod.uid", CONTAINER_NAME_PREFIX),
            self.metadata.uid.unwrap().to_string(),
        );
        labels
    }

    pub async fn reconcile(&self) -> Result<()> {
        let actions = self.compute_pod_actions().await;
        docker::remove_containers(&actions.containers_to_remove).await;
        let sandbox = if actions.create_sandbox {
            self.create_sandbox().await?
        } else {
            self.sandbox()
        };
        if actions.start_sandbox {
            sandbox.start().await?;
        }
        self.start_containers(&actions.containers_to_start, &sandbox)
            .await
    }

    async fn compute_pod_actions(&self) -> PodActions {
        let mut actions = PodActions {
            create_sandbox: false,
            start_sandbox: false,
            containers_to_start: vec![],
            containers_to_remove: vec![],
        };

        let sandbox = self.sandbox();
        let response = sandbox.inspect().await;
        match response {
            Ok(response) => {
                match response {
                    Some(response) => {
                        // If sandbox is not running, restart it
                        if let ContainerState::Terminated {
                            ..
                        } = ContainerState::from(response.state)
                        {
                            tracing::info!("Sandbox is not running, restarting...");
                            actions.start_sandbox = true;
                        }
                    },
                    // Sandbox doesn't exist, create it,
                    // all containers need to be removeed and created
                    None => {
                        tracing::info!("Sandbox is missing, recreating...");
                        actions.create_sandbox = true;
                        actions.containers_to_remove.append(&mut self.containers());
                        actions
                            .containers_to_start
                            .append(&mut self.spec.containers.clone());
                        return actions;
                    },
                }
            },
            Err(_) => {
                // Inspect error, assume sandbox is dead, remove and recreate it
                actions.create_sandbox = true;
                actions.containers_to_remove.push(sandbox);
                actions.containers_to_remove.append(&mut self.containers());
                actions
                    .containers_to_start
                    .append(&mut self.spec.containers.clone());
                return actions;
            },
        }

        for container in self.spec.containers.iter() {
            let status = self
                .status
                .container_statuses
                .iter()
                .find(|status| status.name == container.name);
            // If status was not found (container is missing) or container is not running
            if status.map(|status| status.state == ContainerState::Running) != Some(true)
                && self.should_restart_container(status)
            {
                tracing::info!("Container {} is not running, restarting...", container.name);
                actions.containers_to_start.push(container.to_owned());
            }
        }
        actions
    }

    fn should_restart_container(&self, status: Option<&ContainerStatus>) -> bool {
        match status {
            Some(status) => match status.state {
                ContainerState::Running | ContainerState::Waiting => false,
                ContainerState::Terminated {
                    exit_code,
                } => match self.spec.restart_policy {
                    RestartPolicy::Always => true,
                    RestartPolicy::Never => false,
                    RestartPolicy::OnFailure => exit_code != 0,
                },
            },
            None => true,
        }
    }
}
