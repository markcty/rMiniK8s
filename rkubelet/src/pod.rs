use std::default::Default;

use bollard::{
    container::Config,
    models::{ContainerInspectResponse, HostConfig},
};
use futures::future::try_join_all;
use resources::objects::{
    pod,
    pod::{ContainerStatus, PodSpec, PodStatus},
    KubeObject, KubeResource, Metadata,
};
use uuid::Uuid;

use crate::{
    config::{CONTAINER_NAME_PREFIX, PAUSE_CONTAINER_NAME, PAUSE_IMAGE_NAME},
    docker::{Container, Image},
};

#[derive(Debug)]
#[allow(dead_code)]
pub struct Pod {
    metadata: Metadata,
    spec: PodSpec,
    status: PodStatus,
}

impl Pod {
    #[allow(dead_code)]
    pub fn load(object: KubeObject) -> Self {
        if let KubeResource::Pod(resource) = object.resource {
            let status = resource.status.expect("Pod::load:: status is missing");
            Pod {
                metadata: object.metadata,
                spec: resource.spec,
                status,
            }
        } else {
            panic!("Expecting Pod, received {:?}", object.resource);
        }
    }

    pub async fn create(object: KubeObject) -> Result<Self, bollard::errors::Error> {
        if let KubeResource::Pod(resource) = object.resource {
            tracing::info!("Creating pod containers...");

            let mut metadata = object.metadata;
            let uid = Uuid::new_v4();
            metadata.uid = Some(uid);

            let mut pod = Self {
                metadata,
                spec: resource.spec,
                status: PodStatus::default(),
            };
            let pause_container = pod.create_pause_container().await?;
            pod.create_containers(&pause_container).await?;
            pod.update_status().await?;

            Ok(pod)
        } else {
            panic!("Expecting Pod, received {:?}", object.resource);
        }
    }

    #[allow(dead_code)]
    pub async fn start(&self) -> Result<(), bollard::errors::Error> {
        tracing::info!("Starting pod containers...");
        let containers = self
            .status
            .container_statuses
            .iter()
            .map(Container::from)
            .collect::<Vec<Container>>();
        self.start_containers(&containers)
            .await
            .map(|_| tracing::info!("Pod created"))
    }

    async fn create_container(
        &self,
        container: &pod::Container,
        pause_container: &Container,
    ) -> Result<Container, bollard::errors::Error> {
        let image = Image::create(&container.image).await;
        let mode = Some(format!("container:{}", pause_container.id()));
        // TODO: Handle volume mounts
        let host_config = Some(HostConfig {
            cpu_shares: Some(container.resources.limits.cpu),
            memory: Some(container.resources.limits.memory),
            network_mode: mode.to_owned(),
            ipc_mode: mode.to_owned(),
            pid_mode: mode.to_owned(),
            ..Default::default()
        });
        let config = Config {
            image: Some(image.name().to_owned()),
            entrypoint: Some(container.command.to_owned()),
            host_config,
            ..Default::default()
        };
        let name = Some(self.unique_container_name(&container.name));
        Container::create(name, config).await
    }

    async fn create_containers(
        &self,
        pause_container: &Container,
    ) -> Result<Vec<Container>, bollard::errors::Error> {
        let mut tasks = vec![];
        for container in &self.spec.containers {
            tasks.push(self.create_container(container, pause_container));
        }
        try_join_all(tasks).await
    }

    async fn create_pause_container(&self) -> Result<Container, bollard::errors::Error> {
        let image = Image::create(PAUSE_IMAGE_NAME).await;
        let host_config = Some(HostConfig {
            network_mode: Some(self.spec.network_mode()),
            ipc_mode: Some("shareable".to_string()),
            ..Default::default()
        });
        let config = Config {
            image: Some(image.name().to_owned()),
            host_config,
            ..Default::default()
        };
        let name = Some(self.unique_container_name(PAUSE_CONTAINER_NAME));
        let container = Container::create(name, config).await?;
        container.start().await?;
        Ok(container)
    }

    async fn start_containers(
        &self,
        containers: &[Container],
    ) -> Result<Vec<()>, bollard::errors::Error> {
        let mut tasks = vec![];
        for container in containers {
            tasks.push(container.start());
        }
        try_join_all(tasks).await
    }

    async fn inspect_containers(
        &self,
    ) -> Result<Vec<ContainerInspectResponse>, bollard::errors::Error> {
        let containers = self.containers();
        let tasks = containers.iter().map(|c| c.inspect()).collect::<Vec<_>>();
        try_join_all(tasks).await
    }

    pub async fn update_status(&mut self) -> Result<(), bollard::errors::Error> {
        let response = self.pause_container().inspect().await?;
        let pod_ip = Pod::get_ip(&response).map(|ip| ip.to_owned());
        // TODO: strip uid off container names
        let container_statuses = self
            .inspect_containers()
            .await?
            .into_iter()
            .map(ContainerStatus::from)
            .collect();
        // TODO: determine pod conditions
        self.status = PodStatus {
            pod_ip,
            container_statuses,
            ..Default::default()
        };
        Ok(())
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

    fn unique_container_name(&self, container_name: &str) -> String {
        let uid = self.metadata.uid.expect("Pod with no uid");
        format!("{}_{}-{}", CONTAINER_NAME_PREFIX, container_name, uid)
    }

    fn pause_container(&self) -> Container {
        Container::new(self.unique_container_name(PAUSE_CONTAINER_NAME))
    }

    fn containers(&self) -> Vec<Container> {
        self.spec
            .containers
            .iter()
            .map(|container| Container::new(self.unique_container_name(&container.name)))
            .collect()
    }
}
