use std::{fs::File, path::PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use reqwest::Client;
use serde::Deserialize;

use crate::{objects::KubeObject, utils::gen_url_from_object};

#[derive(Args)]
pub struct Arg {
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    file: PathBuf,
}

impl Arg {
    pub async fn handle(&self) -> Result<()> {
        let path = &self.file.as_path();
        let file =
            File::open(path).with_context(|| format!("Failed to open file {}", path.display()))?;
        let object: KubeObject = serde_yaml::from_reader(file)
            .with_context(|| format!("Failed to parse file {}", path.display()))?;
        let msg = patch(&object)
            .await
            .with_context(|| format!("Failed to patch using file {}", path.display()))?;

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

#[derive(Debug, Deserialize)]
struct PatchRes {
    msg: String,
}
