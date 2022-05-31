use std::{cmp::Eq, collections::HashMap, default::Default, hash::Hash, pin::Pin};

use anyhow::{Context, Result};
use bollard::{
    container::{Config, CreateContainerOptions, LogsOptions, StartContainerOptions},
    errors::Error::DockerResponseServerError,
    exec::{CreateExecOptions, StartExecResults},
    image::{CreateImageOptions, ListImagesOptions},
    models::ContainerInspectResponse,
};
use chrono::Utc;
use futures::{future::join_all, Stream, StreamExt};
use resources::objects::pod::{ContainerStatus, ImagePullPolicy};
use serde::Serialize;
use tokio::io::AsyncWrite;

use crate::config::DOCKER;

#[derive(Debug)]
pub struct Image {
    name: String,
}

impl Image {
    pub fn new(name: &str) -> Self {
        let name = if name.contains(':') {
            name.to_string()
        } else {
            // Default tag to "latest", otherwise we'll pull all the tags
            format!("{}:latest", name)
        };
        Self {
            name,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub async fn pull(&self) -> Result<()> {
        let options = Some(CreateImageOptions {
            from_image: self.name.clone(),
            ..Default::default()
        });
        let mut stream = DOCKER.create_image(options, None, None);

        tracing::info!("Pulling image {}...", self.name);
        while let Some(result) = stream.next().await {
            let result = result?;
            if let Some(error) = result.error {
                tracing::error!("{:#}", error);
            }
            if let Some(progress) = result.progress {
                tracing::info!("{}", progress);
            }
        }
        Ok(())
    }

    pub async fn exists(&self) -> bool {
        let mut filters = HashMap::new();
        filters.insert("reference".to_string(), vec![self.name.clone()]);
        let options = Some(ListImagesOptions {
            filters,
            ..Default::default()
        });
        let response = DOCKER.list_images(options).await;
        match response {
            Ok(images) => !images.is_empty(),
            Err(error) => {
                tracing::error!("Error listing image: {:#}", error);
                false
            },
        }
    }

    pub async fn pull_with_policy(&self, policy: ImagePullPolicy) -> Result<()> {
        match policy {
            ImagePullPolicy::IfNotPresent => {
                if self.exists().await {
                    Ok(())
                } else {
                    self.pull().await
                }
            },
            ImagePullPolicy::Always => self.pull().await,
            ImagePullPolicy::Never => Ok(()),
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

    /// Inspect a container, return `None` if not found
    pub async fn inspect(&self) -> Result<Option<ContainerInspectResponse>> {
        let response = DOCKER.inspect_container(self.id.as_str(), None).await;
        match response {
            Ok(response) => Ok(Some(response)),
            Err(err) => {
                if let DockerResponseServerError {
                    status_code, ..
                } = err
                {
                    if status_code.eq(&404) {
                        return Ok(None);
                    }
                }
                Err(err).with_context(|| format!("Failed to inspect container {}", self.id))
            },
        }
    }

    pub async fn stop(&self) -> Result<()> {
        DOCKER
            .stop_container(self.id.as_str(), None)
            .await
            .with_context(|| format!("Failed to stop container {}", self.id))
    }

    /// Remove a container, return `true` if succeeded, `false` if not found
    pub async fn remove(&self) -> Result<bool> {
        let response = DOCKER.remove_container(self.id.as_str(), None).await;
        match response {
            Ok(_) => Ok(true),
            Err(err) => {
                if let DockerResponseServerError {
                    status_code, ..
                } = err
                {
                    if status_code.eq(&404) {
                        return Ok(false);
                    }
                }
                Err(err).with_context(|| format!("Failed to inspect container {}", self.id))
            },
        }
    }

    pub async fn logs(&self, tail: &Option<String>) -> Result<String> {
        let tail = tail.to_owned().unwrap_or_else(|| "all".to_string());
        let mut stream = DOCKER.logs(
            self.id.as_str(),
            Some(LogsOptions {
                stdout: true,
                stderr: true,
                tail,
                until: Utc::now().timestamp(),
                ..Default::default()
            }),
        );
        let mut logs = String::new();
        while let Some(log) = stream.next().await {
            match log {
                Ok(log) => {
                    let log = log.into_bytes();
                    match std::str::from_utf8(&log) {
                        Ok(log) => logs += log,
                        Err(err) => {
                            tracing::error!("{:#}", err);
                        },
                    }
                },
                Err(err) => {
                    tracing::error!("{:#}", err);
                    return Err(anyhow::anyhow!("Failed to get logs: {:#}", err));
                },
            }
        }
        Ok(logs)
    }

    pub async fn exec(
        &self,
        command: &str,
    ) -> Result<(
        Pin<
            Box<
                dyn Stream<Item = Result<bollard::container::LogOutput, bollard::errors::Error>>
                    + Send,
            >,
        >,
        Pin<Box<dyn AsyncWrite + Send>>,
    )> {
        let exec = DOCKER
            .create_exec(
                self.id.as_str(),
                CreateExecOptions {
                    cmd: Some(command.split(' ').collect()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(true),
                    tty: Some(true),
                    ..Default::default()
                },
            )
            .await?
            .id;
        if let StartExecResults::Attached {
            output,
            input,
        } = DOCKER.start_exec(&exec, None).await?
        {
            Ok((output, input))
        } else {
            Err(anyhow::anyhow!("Failed to start exec"))
        }
    }
}

impl From<&ContainerStatus> for Container {
    fn from(status: &ContainerStatus) -> Self {
        Self {
            id: status.container_id.to_owned(),
        }
    }
}

/// Start docker containers concurrently
pub async fn start_containers(containers: &[Container]) -> Vec<Result<()>> {
    let tasks = containers.iter().map(|c| c.start()).collect::<Vec<_>>();
    let results = join_all(tasks).await;
    results.iter().for_each(|r| {
        if let Err(e) = r {
            tracing::error!("Failed to start container: {:#}", e);
        }
    });
    results
}

/// Stop docker containers concurrently
pub async fn stop_containers(containers: &[Container]) -> Vec<Result<()>> {
    let tasks = containers.iter().map(|c| c.stop()).collect::<Vec<_>>();
    let results = join_all(tasks).await;
    results.iter().for_each(|r| {
        if let Err(e) = r {
            tracing::error!("Failed to stop container: {:#}", e);
        }
    });
    results
}

/// Remove docker containers concurrently
pub async fn remove_containers(containers: &[Container]) -> Vec<Result<bool>> {
    let tasks = containers.iter().map(|c| c.remove()).collect::<Vec<_>>();
    let results = join_all(tasks).await;
    results.iter().for_each(|r| {
        if let Err(e) = r {
            tracing::error!("Failed to remove container: {:#}", e);
        }
    });
    results
}
