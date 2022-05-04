use anyhow::Result;
use chrono::Local;
use chrono_humanize::{Accuracy, HumanTime, Tense};
use clap::Args;
use reqwest::blocking::Client;
use resources::{
    models::Response,
    objects::{KubeObject, KubeResource::Pod},
};

use crate::utils::gen_url;

#[derive(Args)]
pub struct Arg {
    /// Kind of resource
    kind: String,
    /// Name of resource
    name: Option<String>,
}

impl Arg {
    pub fn handle(&self) -> Result<()> {
        let client = Client::new();
        let url = gen_url(self.kind.to_owned(), self.name.to_owned())?;
        let res = client
            .get(url)
            .send()?
            .json::<Response<Vec<KubeObject>>>()?;

        println!("{: <20} {: <8} {: <10}", "NAME", "STATUS", "AGE");
        for object in res.data.unwrap() {
            if let Pod(pod) = object.resource {
                let d = HumanTime::from(
                    Local::now().naive_utc() - pod.status.as_ref().unwrap().start_time,
                );
                println!(
                    "{: <20} {: <8} {: <10}",
                    object.metadata.name,
                    pod.status.unwrap().phase,
                    d.to_text_en(Accuracy::Rough, Tense::Present)
                );
            }
        }
        Ok(())
    }
}
