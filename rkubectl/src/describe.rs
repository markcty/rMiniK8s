use std::vec::Vec;

use anyhow::Result;
use clap::Args;
use reqwest::blocking::Client;
use resources::{models::Response, objects::KubeObject};

use crate::{utils::gen_url, ResourceKind};

#[derive(Args)]
pub struct Arg {
    /// Kind of resource
    #[clap(arg_enum)]
    kind: ResourceKind,
    /// Name of resource
    name: Option<String>,
}

impl Arg {
    pub fn handle(&self) -> Result<()> {
        let client = Client::new();
        let url = gen_url(self.kind.to_string(), self.name.as_ref())?;
        let data = if self.name.is_none() {
            let res = client
                .get(url)
                .send()?
                .json::<Response<Vec<KubeObject>>>()?;
            res.data.unwrap_or_default()
        } else {
            let res = client.get(url).send()?.json::<Response<KubeObject>>()?;
            res.data.map_or_else(Vec::new, |data| vec![data])
        };

        for object in data {
            match object {
                KubeObject::Pod(pod) => {
                    println!("{}", pod);
                },
                KubeObject::ReplicaSet(rs) => {
                    println!("{}", rs);
                },
                KubeObject::Node(node) => {
                    println!("{}", node);
                },
                _ => {
                    println!("{:#?}", object);
                },
            }
        }
        Ok(())
    }
}
