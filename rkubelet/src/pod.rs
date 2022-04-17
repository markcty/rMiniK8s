use std::default::Default;

use bollard::{
    container::Config,
    models::{ContainerInspectResponse, HostConfig},
};
use futures::future::try_join_all;
use resources::objects::{
    pod,
    pod::{ContainerStatus, PodSpec, PodStatus},
    KubeObject, KubeSpec, KubeStatus,
};

use crate::docker::{Container, Image};

#[derive(Debug)]
#[allow(dead_code)]
pub struct Pod {
    spec: PodSpec,
    status: PodStatus,
}

impl Pod {
    #[allow(dead_code)]
    pub fn load(object: KubeObject) -> Self {
        // TODO: Change to `match` after adding more types
        let KubeSpec::Pod(spec) = object.spec;
        let status = match object.status {
            Some(status) => match status {
                KubeStatus::Pod(status) => status,
                // _ => panic!("Pod::load: object.status is not a Pod"),
            },
            None => panic!("Pod::load:: status is missing"),
        };
        Pod {
            spec,
            status,
        }
    }

    pub async fn create(object: KubeObject) -> Result<Self, bollard::errors::Error> {
        // TODO: Change to `match` after adding more types
        let KubeSpec::Pod(spec) = object.spec;

        tracing::info!("Creating pod containers...");
        let pause_container = Pod::create_pause_container().await?;
        let containers = Pod::create_containers(&spec.containers, &pause_container).await?;
        let status = Pod::get_status(&pause_container, &containers).await?;

        Ok(Self {
            spec,
            status,
        })
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
        container: &pod::Container,
        pause_container: &Container,
    ) -> Result<Container, bollard::errors::Error> {
        let image = Image::create(&container.image).await;
        let mode = Some(format!("container:{}", pause_container.id()));
        let host_config = Some(HostConfig {
            network_mode: mode.to_owned(),
            ipc_mode: mode.to_owned(),
            pid_mode: mode.to_owned(),
            ..Default::default()
        });
        let config = Config {
            image: Some(image.name().to_owned()),
            host_config,
            ..Default::default()
        };
        Container::create(Some(&container.name), config).await
    }

    async fn create_containers(
        containers: &[pod::Container],
        pause_container: &Container,
    ) -> Result<Vec<Container>, bollard::errors::Error> {
        let mut tasks = vec![];
        for container in containers {
            tasks.push(Pod::create_container(container, pause_container));
        }
        try_join_all(tasks).await
    }

    async fn create_pause_container() -> Result<Container, bollard::errors::Error> {
        let image = Image::create("kubernetes/pause:latest").await;
        let host_config = Some(HostConfig {
            ipc_mode: Some("shareable".to_string()),
            ..Default::default()
        });
        let config = Config {
            image: Some(image.name().to_owned()),
            host_config,
            ..Default::default()
        };
        let container = Container::create(Some("pause"), config).await?;
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
        containers: &[Container],
    ) -> Result<Vec<ContainerInspectResponse>, bollard::errors::Error> {
        let mut tasks = vec![];
        for container in containers {
            tasks.push(container.inspect());
        }
        try_join_all(tasks).await
    }

    async fn get_status(
        pause_container: &Container,
        containers: &[Container],
    ) -> Result<PodStatus, bollard::errors::Error> {
        let response = pause_container.inspect().await?;
        let pod_ip = Pod::get_ip(&response).map(|ip| ip.to_owned());
        let container_statuses = Pod::inspect_containers(containers)
            .await?
            .into_iter()
            .map(ContainerStatus::from)
            .collect();
        Ok(PodStatus {
            pod_ip,
            container_statuses,
            ..Default::default()
        })
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
}
