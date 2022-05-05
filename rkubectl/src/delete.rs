use anyhow::Result;
use clap::Args;
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
    pub fn handle(&self) -> Result<()> {
        let client = reqwest::blocking::Client::new();
        let url = gen_url(self.kind.to_string(), Some(&self.name))?;
        let res = client.delete(url).send()?.json::<DeleteRes>()?;
        println!("{}", res.msg);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct DeleteRes {
    msg: String,
}
