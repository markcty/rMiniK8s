use std::default::Default;

use bollard::{
    container::{Config, CreateContainerOptions, StartContainerOptions},
    image::CreateImageOptions,
    models::HostConfig,
    Docker,
};
use futures::{future::try_join_all, StreamExt};
use resources::objects::{pod::*, KubeObject, KubeSpec};

pub struct PodManager {
    docker: Docker,
}

impl PodManager {
    pub fn new() -> Self {
        Self {
            docker: Docker::connect_with_local_defaults()
                .expect("Failed to connect to docker daemon"),
        }
    }

    async fn pull_image(&self, image_name: &String) {
        let options = Some(CreateImageOptions {
            from_image: image_name.to_owned(),
            ..Default::default()
        });
        let mut stream = self.docker.create_image(options, None, None);

        tracing::info!("Pulling image {}...", image_name);
        while let Some(result) = stream.next().await {
            if let Some(v) = result.unwrap().progress {
                tracing::info!("{}", v);
            }
        }
    }

    async fn create_container(
        &self,
        container: &Container,
        pause_container: &String,
    ) -> Result<String, bollard::errors::Error> {
        self.pull_image(&container.image).await;
        let options = Some(CreateContainerOptions {
            name: &container.name,
        });
        let mode = Some(format!("container:{}", pause_container));
        let host_config = Some(HostConfig {
            network_mode: mode.to_owned(),
            ipc_mode: mode.to_owned(),
            pid_mode: mode.to_owned(),
            ..Default::default()
        });
        let config = Config {
            image: Some(container.image.to_owned()),
            host_config,
            ..Default::default()
        };
        self.docker
            .create_container(options, config)
            .await
            .map(|response| response.id)
    }

    async fn create_containers(
        &self,
        containers: &Vec<Container>,
        pause_container: &String,
    ) -> Result<Vec<String>, bollard::errors::Error> {
        let mut tasks = vec![];
        for container in containers {
            tasks.push(self.create_container(container, pause_container));
        }
        try_join_all(tasks).await
    }

    async fn create_pause_container(&self) -> Result<String, bollard::errors::Error> {
        let options = Some(CreateContainerOptions {
            name: "pause",
        });
        let host_config = Some(HostConfig {
            ipc_mode: Some("shareable".to_string()),
            ..Default::default()
        });
        let config = Config {
            image: Some("kubernetes/pause:latest"),
            host_config,
            ..Default::default()
        };
        let container_id = self
            .docker
            .create_container(options, config)
            .await
            .map(|response| response.id)?;
        self.docker
            .start_container("pause", None::<StartContainerOptions<String>>)
            .await
            .and(Ok(container_id))
    }

    async fn start_containers(
        &self,
        containers: &Vec<String>,
    ) -> Result<Vec<()>, bollard::errors::Error> {
        let mut tasks = vec![];
        for container in containers {
            tasks.push(
                self.docker
                    .start_container(container, None::<StartContainerOptions<String>>),
            );
        }
        try_join_all(tasks).await
    }

    pub async fn create(&self, pod: &KubeObject) -> Result<(), bollard::errors::Error> {
        assert!(matches!(&pod.spec, KubeSpec::Pod(_)));
        let KubeSpec::Pod(spec) = &pod.spec;
        tracing::info!("Creating pod containers...");
        let pause_container = self.create_pause_container().await?;
        let container_ids = self
            .create_containers(&spec.containers, &pause_container)
            .await?;
        tracing::info!("Starting pod containers...");
        self.start_containers(&container_ids)
            .await
            .map(|_| tracing::info!("Pod created"))
    }
}
