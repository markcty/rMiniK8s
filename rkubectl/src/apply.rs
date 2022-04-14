use std::fs::File;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use serde::Deserialize;

use crate::objects::KubeObject;
use crate::{Url, CONFIG};

#[derive(Args)]
pub struct Arg {
    #[clap(short, long, parse(from_os_str), value_name = "FILE")]
    file: PathBuf,
}

impl Arg {
    pub fn handle(&self) -> Result<()> {
        let path = &self.file.as_path();
        let file =
            File::open(path).with_context(|| format!("Failed to open file {}", path.display()))?;
        let object: KubeObject = serde_yaml::from_reader(file)
            .with_context(|| format!("Failed to parse file {}", path.display()))?;
        let msg =
            apply(&object).with_context(|| format!("Failed to apply file {}", path.display()))?;

        println!("{}", msg);
        Ok(())
    }
}

fn apply(object: &KubeObject) -> Result<String> {
    let client = reqwest::blocking::Client::new();
    let url = gen_url(object)?;
    let res = client.post(url).json(&object).send()?.json::<ApplyRes>()?;
    Ok(res.msg)
}

#[derive(Debug, Deserialize)]
struct ApplyRes {
    msg: String,
}

fn gen_url(object: &KubeObject) -> Result<Url> {
    let url = CONFIG.base_url.to_owned();
    let path = format!(
        "{}/{}",
        object.kind.to_lowercase(),
        object.metadata.name.to_lowercase()
    );
    Ok(url.join(path.as_str())?)
}
