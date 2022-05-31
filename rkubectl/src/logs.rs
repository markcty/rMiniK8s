use anyhow::Result;
use clap::Args;
use reqwest::Client;
use resources::models::{ErrResponse, Response};
use serde::Deserialize;

use crate::utils::gen_url;

#[derive(Args)]
pub struct Arg {
    /// Pod name
    pod_name: String,
    /// Container name, optional if there's only one container
    container_name: Option<String>,
    /// Tail option
    #[clap(short, long)]
    tail: Option<String>,
}

impl Arg {
    pub async fn handle(&self) -> Result<()> {
        let client = Client::new();
        let base_url = gen_url("pods".to_string(), Some(&self.pod_name))?;
        let url = match self.container_name {
            Some(ref container_name) => format!("{base_url}/containers/{container_name}/logs"),
            None => format!("{base_url}/logs"),
        };
        let res = client
            .get(url)
            .query(&[("tail", &self.tail.as_ref().unwrap_or(&"all".to_string()))])
            .send()
            .await?
            .json::<LogsResponse>()
            .await?;
        match res {
            LogsResponse::Ok(res) => print!("{}", res.data.unwrap_or_default()),
            LogsResponse::Err(res) => println!("{}: {}", res.msg, res.cause.unwrap_or_default()),
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LogsResponse {
    // NOTE: must place error first
    // otherwise serde will incorrectly match ErrResponse as success
    Err(ErrResponse),
    Ok(Response<String>),
}
