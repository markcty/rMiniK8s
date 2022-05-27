use std::{
    fs::{self, File},
    io::{ErrorKind, Write},
    process::{Command, Stdio},
};

use anyhow::{anyhow, Context, Error, Result};
use fs_extra::dir::{copy, CopyOptions};
use reqwest::Client;
use resources::{
    models::Response,
    objects::{function::Function, replica_set::ReplicaSet, KubeObject, Object},
};
use tokio::{
    select,
    sync::{mpsc, mpsc::Receiver},
    task::JoinHandle,
};

use crate::{
    utils::{create_informer, log_command, Event, ResyncNotification},
    CONFIG, DOCKER_REGISTRY, TEMPLATES_DIR, TMP_DIR,
};

pub struct FunctionController {
    client: Client,
    func_rx: Receiver<Event<Function>>,
    func_resync_rx: Receiver<ResyncNotification>,
    func_informer: Option<JoinHandle<Result<(), Error>>>,
}

impl FunctionController {
    pub fn new() -> Self {
        let (func_tx, func_rx) = mpsc::channel::<Event<Function>>(16);
        let (func_resync_tx, func_resync_rx) = mpsc::channel::<ResyncNotification>(16);
        let func_informer =
            create_informer::<Function>("functions".to_string(), func_tx, func_resync_tx);

        let func_informer = tokio::spawn(async move { func_informer.run().await });

        Self {
            client: Client::new(),
            func_rx,
            func_resync_rx,
            func_informer: Some(func_informer),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("Function Controller started");

        loop {
            select! {
                Some(event) = self.func_rx.recv() => {
                    let result = match event {
                        Event::Add(func) => self.handle_function_add(func).await,

                        _ => {Ok(())}
                    };
                    if let Err(e) = result {
                        tracing::error!("Error while processing Function update: {:#}", e);
                    }
                },
                _ = self.func_resync_rx.recv() => {}
                else => break
            }
        }

        let func_informer = std::mem::replace(&mut self.func_informer, None);
        func_informer.unwrap().await??;
        tracing::info!("Function Controller exited");
        Ok(())
    }

    async fn handle_function_add(&mut self, mut func: Function) -> Result<()> {
        tracing::info!("New function: {}", func.metadata.name);
        self.build_function_image(&mut func).await?;
        self.create_replica_set(&func).await?;
        Ok(())
    }

    async fn build_function_image(&self, func: &mut Function) -> Result<()> {
        self.prepare_file(func).await?;
        let image_name = self.build_image(func).await?;

        func.status.image = Some(image_name);

        self.post_status(func).await?;

        Ok(())
    }

    async fn prepare_file(&self, func: &Function) -> Result<()> {
        let func_dir_path = std::path::Path::new(TMP_DIR).join(func.name());
        self.fetch_code(func).await?;
        // unzip
        Command::new("unzip")
            .current_dir(&func_dir_path)
            .args(["-o", func.spec.filename.as_str()])
            .output()?;
        // copy templates
        let mut copy_option = CopyOptions::new();
        copy_option.content_only = true;
        copy_option.overwrite = true;
        copy(TEMPLATES_DIR, func_dir_path, &copy_option)?;
        Ok(())
    }

    async fn fetch_code(&self, func: &Function) -> Result<()> {
        let client = reqwest::Client::new();
        let filename = func.spec.filename.to_owned();
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

        let func_dir_path = std::path::Path::new(TMP_DIR).join(func.name());
        fs::create_dir_all(&func_dir_path)?;
        let code_path = func_dir_path.join(filename);
        let mut file = File::create(code_path)?;
        let content = response.bytes().await?;
        file.write_all(content.as_ref())?;

        Ok(())
    }

    async fn build_image(&self, func: &Function) -> Result<String> {
        let tag = format!("{}/{}:latest", DOCKER_REGISTRY, func.name());
        tracing::info!("building image: {}", tag);
        let func_dir_path = std::path::Path::new(TMP_DIR).join(func.name());
        let output = Command::new("docker")
            .current_dir(&func_dir_path)
            .args(["build", "-t", tag.as_str(), "."])
            .stdout(Stdio::piped())
            .spawn()?
            .stdout
            .ok_or_else(|| {
                std::io::Error::new(ErrorKind::Other, "Could not capture standard output.")
            })?;
        log_command(output);
        let output = Command::new("docker")
            .current_dir(&func_dir_path)
            .args(["push", tag.as_str()])
            .stdout(Stdio::piped())
            .spawn()?
            .stdout
            .ok_or_else(|| {
                std::io::Error::new(ErrorKind::Other, "Could not capture standard output.")
            })?;
        log_command(output);

        Ok(tag)
    }

    async fn post_status(&self, func: &Function) -> Result<()> {
        let name = func.metadata.name.to_owned();
        let response = self
            .client
            .put(format!("{}{}", CONFIG.api_server_url, func.uri()))
            .json(&KubeObject::Function(func.to_owned()))
            .send()
            .await?
            .json::<Response<()>>()
            .await
            .with_context(|| "Error posting status")?;
        tracing::info!(
            "Posted status for Function {}: {}",
            name,
            response.msg.unwrap_or_else(|| "".to_string())
        );
        Ok(())
    }

    async fn create_replica_set(&self, func: &Function) -> Result<()> {
        let rs = ReplicaSet::from_function(func);
        let response = self
            .client
            .post(format!("{}{}", CONFIG.api_server_url, rs.prefix()))
            .json(&KubeObject::ReplicaSet(rs))
            .send()
            .await?
            .json::<Response<()>>()
            .await
            .with_context(|| "Error creating replicaset")?;
        tracing::info!(
            "Created replicaset for Function {}: {}",
            func.metadata.name,
            response.msg.unwrap_or_else(|| "".to_string())
        );
        Ok(())
    }
}
