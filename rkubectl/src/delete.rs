use anyhow::Result;
use clap::Args;
use reqwest::Client;
use serde::Deserialize;

use crate::{utils::gen_url, ResourceKind};

#[derive(Args)]
pub struct Arg {
    /// Kind of resource
    #[clap(arg_enum)]
    kind: ResourceKind,
    /// Name of resource
    name: String,
}

impl Arg {
    pub async fn handle(&self) -> Result<()> {
        let client = Client::new();
        let url = gen_url(self.kind.to_string(), Some(&self.name))?;
        let res = client.delete(url).send().await?.json::<DeleteRes>().await?;
        println!("{}", res.msg);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct DeleteRes {
    msg: String,
}
