use std::{cmp::Eq, default::Default, hash::Hash};

use bollard::{
    container::{Config, CreateContainerOptions, StartContainerOptions},
    image::CreateImageOptions,
};
use futures::StreamExt;
use resources::objects::pod::ContainerStatus;
use serde::Serialize;

use crate::config::DOCKER;

#[derive(Debug)]
pub struct Image {
    name: String,
}

impl Image {
    pub fn name(&self) -> &String {
        &self.name
    }

    pub async fn create(name: &str) -> Self {
        let options = Some(CreateImageOptions {
            from_image: name,
            ..Default::default()
        });
        let mut stream = DOCKER.create_image(options, None, None);

        tracing::info!("Pulling image {}...", name);
        while let Some(result) = stream.next().await {
            let result = result.unwrap();
            if let Some(error) = result.error {
                tracing::error!("{}", error);
            }
            if let Some(progress) = result.progress {
                tracing::info!("{}", progress);
            }
        }
        Image {
            name: name.to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Container {
    /// ID or name of the container
    id: String,
}

impl Container {
    pub fn new(id: String) -> Self {
        Self {
            id,
        }
    }

    pub fn id(&self) -> &String {
        &self.id
    }

    pub async fn create<T, Z>(
        name: Option<T>,
        config: Config<Z>,
    ) -> Result<Self, bollard::errors::Error>
    where
        T: Into<String> + Serialize,
        Z: Into<String> + Hash + Eq + Serialize,
    {
        let options = name.map(|name| CreateContainerOptions {
            name,
        });
        DOCKER
            .create_container(options, config)
            .await
            .map(|response| {
                for warning in response.warnings {
                    tracing::warn!("{}", warning);
                }
                Self {
                    id: response.id,
                }
            })
    }

    pub async fn start(&self) -> Result<(), bollard::errors::Error> {
        DOCKER
            .start_container(self.id.as_str(), None::<StartContainerOptions<String>>)
            .await
    }

    pub async fn inspect(
        &self,
    ) -> Result<bollard::models::ContainerInspectResponse, bollard::errors::Error> {
        DOCKER.inspect_container(self.id.as_str(), None).await
    }
}

impl From<&ContainerStatus> for Container {
    fn from(status: &ContainerStatus) -> Self {
        Self {
            id: status.container_id.to_owned(),
        }
    }
}
