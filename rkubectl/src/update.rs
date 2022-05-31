use std::{fs::File, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::Args;
use reqwest::blocking::multipart;
use resources::objects::Object;
use serde::Deserialize;

use crate::{objects::KubeObject, utils::gen_url};

#[derive(Args)]
pub struct Arg {
    /// The definition YAML file of the resource to create
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    file: PathBuf,
    /// ZIP code file to upload, required when creating jobs and functions
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
                update_with_file(&object, code_path)
                    .with_context(|| format!("Failed to update using file {}", path.display()))?
            },
            _ => update(&object)
                .with_context(|| format!("Failed to update using file {}", path.display()))?,
        };

        println!("{}", msg);
        Ok(())
    }
}

fn update(object: &KubeObject) -> Result<String> {
    let client = reqwest::blocking::Client::new();
    let url = gen_url(object.kind_plural(), None)?;
    let res = client.put(url).json(&object).send()?.json::<UpdateRes>()?;
    match res.cause {
        Some(cause) => Err(anyhow::anyhow!("{}: {}", res.msg, cause)),
        None => Ok(res.msg),
    }
}

fn update_with_file(object: &KubeObject, file: PathBuf) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .pool_idle_timeout(None)
        .timeout(None)
        .connect_timeout(None)
        .build()?;
    let url = gen_url(object.kind_plural(), None)?;

    let res = client
        .delete(gen_url(object.kind_plural(), Some(object.name()))?)
        .send()?
        .json::<UpdateRes>()?;
    if let Some(cause) = res.cause {
        return Err(anyhow::anyhow!("{}: {}", res.msg, cause));
    }

    let form = multipart::Form::new()
        .text(
            object.kind().to_lowercase(),
            serde_json::to_string(&object)?,
        )
        .file("code", file)?;
    let res = client
        .post(url)
        .multipart(form)
        .send()?
        .json::<UpdateRes>()?;
    match res.cause {
        Some(cause) => Err(anyhow::anyhow!("{}: {}", res.msg, cause)),
        None => Ok(res.msg),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateRes {
    msg: String,
    cause: Option<String>,
}
