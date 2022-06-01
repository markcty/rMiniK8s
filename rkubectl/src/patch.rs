use std::{fs::File, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::Args;
use reqwest::{
    multipart::{self, Part},
    Client,
};
use resources::objects::Object;
use serde::Deserialize;

use crate::{
    objects::KubeObject,
    utils::{gen_url, gen_url_from_object},
};

#[derive(Args)]
pub struct Arg {
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    file: PathBuf,
    #[clap(short, long, parse(from_os_str), value_name = "ZIP")]
    code_file: Option<PathBuf>,
}

impl Arg {
    pub async fn handle(&self) -> Result<()> {
        let path = &self.file.as_path();
        let file =
            File::open(path).with_context(|| format!("Failed to open file {}", path.display()))?;
        let object: KubeObject = serde_yaml::from_reader(file)
            .with_context(|| format!("Failed to parse file {}", path.display()))?;

        let msg: String = match object {
            KubeObject::Function(..) => {
                let code_path = self
                    .code_file
                    .to_owned()
                    .ok_or_else(|| anyhow!("Code file is not provided"))?;
                patch_with_file(&object, code_path)
                    .await
                    .with_context(|| format!("Failed to update using file {}", path.display()))?
            },
            _ => patch(&object)
                .await
                .with_context(|| format!("Failed to patch using file {}", path.display()))?,
        };

        println!("{}", msg);
        Ok(())
    }
}

async fn patch(object: &KubeObject) -> Result<String> {
    let client = Client::new();
    let url = gen_url_from_object(object)?;
    let res = client
        .patch(url)
        .json(&object)
        .send()
        .await?
        .json::<PatchRes>()
        .await?;
    Ok(res.msg)
}

async fn patch_with_file(object: &KubeObject, path: PathBuf) -> Result<String> {
    let client = Client::builder().pool_idle_timeout(None).build()?;
    let url = gen_url(object.kind_plural(), None)?;

    let res = client
        .delete(gen_url(object.kind_plural(), Some(object.name()))?)
        .send()
        .await?
        .json::<PatchRes>()
        .await?;
    if let Some(cause) = res.cause {
        return Err(anyhow::anyhow!("{}: {}", res.msg, cause));
    }

    // Load file as a part
    let bytes = std::fs::read(&path)?;
    let mut file = Part::bytes(bytes);
    if let Some(file_name) = path
        .file_name()
        .map(|filename| filename.to_string_lossy().into_owned())
    {
        file = file.file_name(file_name);
    }

    let form = multipart::Form::new()
        .text(
            object.kind().to_lowercase(),
            serde_json::to_string(&object)?,
        )
        .part("code", file);
    let res = client
        .post(url)
        .multipart(form)
        .send()
        .await?
        .json::<PatchRes>()
        .await?;
    match res.cause {
        Some(cause) => Err(anyhow::anyhow!("{}: {}", res.msg, cause)),
        None => Ok(res.msg),
    }
}

#[derive(Debug, Deserialize)]
struct PatchRes {
    msg: String,
    cause: Option<String>,
}
