use std::{
    collections::HashMap, default::Default, fs::create_dir_all, net::Ipv4Addr, path::PathBuf,
};

use anyhow::{Context, Result};
use bollard::{
    container::Config,
    models::{ContainerInspectResponse, HostConfig},
};
use futures::future::try_join_all;
use resources::objects::{
    pod,
    pod::{
        ContainerState, ContainerStatus, PodCondition, PodConditionType, PodPhase, PodSpec,
        PodStatus, RestartPolicy, VolumeMount,
    },
    KubeObject, KubeResource, Metadata,
};

use crate::{
    config::{CONTAINER_NAME_PREFIX, PAUSE_IMAGE_NAME, POD_DIR_PATH, SANDBOX_NAME},
    docker::{Container, Image},
    volume::Volume,
};

#[derive(Debug)]
pub struct Pod {
    metadata: Metadata,
    spec: PodSpec,
    status: PodStatus,
}

impl Pod {
    pub fn load(object: KubeObject) -> Result<Self> {
        if let KubeResource::Pod(resource) = object.resource {
            let status = resource
                .status
                .with_context(|| "[Pod::load] Status is missing")?;
            Ok(Pod {
                metadata: object.metadata,
                spec: resource.spec,
                status,
            })
        } else {
            Err(anyhow::anyhow!(
                "Expecting Pod, received {:?}",
                object.resource
            ))
        }
    }

    pub async fn create(object: KubeObject) -> Result<Self> {
        if let KubeResource::Pod(resource) = object.resource {
            let name = object.metadata.name.to_owned();
            tracing::info!("Creating pod {}...", name);
            let mut pod = Self {
                metadata: object.metadata,
                spec: resource.spec,
                status: PodStatus::default(),
            };
            create_dir_all(pod.dir())
                .with_context(|| format!("Failed to create pod directory for {}", name))?;

            let sandbox = pod.create_sandbox().await?;
            pod.create_containers(&sandbox).await?;
            pod.update_status().await?;

            Ok(pod)
        } else {
            panic!("Expecting Pod, received {:?}", object.resource);
        }
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting pod {}...", self.metadata.name);
        let containers = self
            .status
            .container_statuses
            .iter()
            .map(Container::from)
            .collect::<Vec<Container>>();
        self.start_containers(&containers)
            .await
            .map(|_| tracing::info!("Pod {} created", self.metadata.name))
    }

    pub async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping pod {}...", self.metadata.name);
        self.stop_containers()
            .await
            .map(|_| tracing::info!("Pod {} stopped", self.metadata.name))
    }

    pub async fn remove(&self) -> Result<()> {
        tracing::info!("Removing pod {}...", self.metadata.name);
        self.stop().await?;
        self.remove_containers()
            .await
            .map(|_| tracing::info!("Pod {} removed", self.metadata.name))
        // TODO: Remove pod directory (DANGEROUS!)
    }

    async fn create_container(
        &self,
        container: &pod::Container,
        sandbox: &Container,
    ) -> Result<Container> {
        let image = Image::create(&container.image).await;
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
        let name = Some(self.unique_container_name(&container.name));
        Container::create(name, config).await
    }

    async fn create_containers(&self, sandbox: &Container) -> Result<Vec<Container>> {
        let mut tasks = vec![];
        for container in &self.spec.containers {
            tasks.push(self.create_container(container, sandbox));
        }
        try_join_all(tasks).await
    }

    async fn create_sandbox(&self) -> Result<Container> {
        let image = Image::create(PAUSE_IMAGE_NAME).await;
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
            ..Default::default()
        };
        let name = Some(self.unique_container_name(SANDBOX_NAME));
        let container = Container::create(name, config).await?;
        container.start().await?;
        Ok(container)
    }

    async fn start_containers(&self, containers: &[Container]) -> Result<()> {
        let mut tasks = vec![];
        for container in containers {
            tasks.push(container.start());
        }
        try_join_all(tasks)
            .await
            .with_context(|| "Failed to start containers")
            .map(|_| ())
    }

    async fn inspect_containers(&self) -> Result<Vec<ContainerInspectResponse>> {
        let containers = self.containers();
        let tasks = containers.iter().map(|c| c.inspect()).collect::<Vec<_>>();
        try_join_all(tasks)
            .await
            .with_context(|| "Failed to inspect containers")
    }

    async fn stop_containers(&self) -> Result<()> {
        let containers = self.containers();
        let tasks = containers.iter().map(|c| c.stop()).collect::<Vec<_>>();
        try_join_all(tasks)
            .await
            .with_context(|| "Failed to stop containers")?;
        self.sandbox().stop().await?;
        Ok(())
    }

    async fn remove_containers(&self) -> Result<()> {
        let containers = self.containers();
        let tasks = containers.iter().map(|c| c.remove()).collect::<Vec<_>>();
        try_join_all(tasks)
            .await
            .with_context(|| "Failed to remove containers")?;
        self.sandbox().remove().await?;
        Ok(())
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
        let pod_ip = Pod::get_ip(&response).map(|ip| ip.parse::<Ipv4Addr>().unwrap());
        let containers = self.inspect_containers().await?;
        let phase = self.compute_phase(&containers);
        let container_statuses = containers
            .into_iter()
            .map(ContainerStatus::from)
            .collect::<Vec<_>>();
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

    fn unique_container_name(&self, container_name: &str) -> String {
        let uid = self.metadata.uid.expect("Pod with no uid");
        format!(
            "{}_{}_{}_{}",
            CONTAINER_NAME_PREFIX, container_name, self.metadata.name, uid
        )
    }

    fn sandbox(&self) -> Container {
        Container::new(self.unique_container_name(SANDBOX_NAME))
    }

    fn containers(&self) -> Vec<Container> {
        self.spec
            .containers
            .iter()
            .map(|container| Container::new(self.unique_container_name(&container.name)))
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
        KubeObject {
            metadata: self.metadata.clone(),
            resource: KubeResource::Pod(pod::Pod {
                spec: self.spec.clone(),
                status: Some(self.status.clone()),
            }),
        }
    }

    fn compute_phase(&self, containers: &Vec<ContainerInspectResponse>) -> PodPhase {
        let mut running = 0;
        let mut waiting = 0;
        let mut stopped = 0;
        let mut succeeded = 0;
        for container in containers {
            let state = ContainerState::from(container.state.to_owned());
            match state {
                ContainerState::Running => running += 1,
                ContainerState::Terminated => {
                    stopped += 1;
                    if let Some(exit_code) =
                        container.state.to_owned().and_then(|state| state.exit_code)
                    {
                        if exit_code == 0 {
                            succeeded += 1;
                        }
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
        let containers_ready = containers
            .iter()
            .all(|status| status.state == ContainerState::Running);
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
}
