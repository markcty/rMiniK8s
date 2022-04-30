use std::{cmp::Eq, default::Default, hash::Hash};

use anyhow::{Context, Result};
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

    pub async fn create<T, Z>(name: Option<T>, config: Config<Z>) -> Result<Self>
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
            .with_context(|| "Failed to create container".to_string())
    }

    pub async fn start(&self) -> Result<()> {
        DOCKER
            .start_container(self.id.as_str(), None::<StartContainerOptions<String>>)
            .await
            .with_context(|| format!("Failed to start container {}", self.id))
    }

    pub async fn inspect(&self) -> Result<bollard::models::ContainerInspectResponse> {
        DOCKER
            .inspect_container(self.id.as_str(), None)
            .await
            .with_context(|| format!("Failed to inspect container {}", self.id))
    }
}

impl From<&ContainerStatus> for Container {
    fn from(status: &ContainerStatus) -> Self {
        Self {
            id: status.container_id.to_owned(),
        }
    }
}
