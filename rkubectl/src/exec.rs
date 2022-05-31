use std::{
    io::{stdout, Read, Write},
    time::Duration,
};

use anyhow::Result;
use clap::Args;
use futures_util::{stream::StreamExt, SinkExt};
use reqwest::Url;
use termion::{async_stdin, raw::IntoRawMode};
use tokio::{spawn, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::CONFIG;

#[derive(Args)]
pub struct Arg {
    /// Pod name
    pod_name: String,
    /// Container name
    container_name: String,
    /// Command to execute
    #[clap(short, long)]
    command: String,
}

impl Arg {
    pub async fn handle(&self) -> Result<()> {
        let mut base_url = CONFIG.base_url.to_owned();
        base_url.set_scheme("ws").unwrap();
        let mut url = Url::parse(
            format!(
                "{}api/v1/pods/{}/containers/{}/exec",
                base_url, self.pod_name, self.container_name
            )
            .as_str(),
        )?;
        url.set_query(Some(&format!("command={}", self.command)));

        let (stream, _) = connect_async(url).await?;
        let (mut sender, mut receiver) = stream.split();
        // Pipe stdin into docker exec input
        spawn(async move {
            let mut stdin = async_stdin().bytes();
            loop {
                if let Some(Ok(byte)) = stdin.next() {
                    sender.send(Message::Binary(vec![byte])).await.ok();
                    if byte == 4 {
                        // Ctrl-D
                        sender.close().await.ok();
                        return;
                    }
                } else {
                    sleep(Duration::from_nanos(10)).await;
                }
            }
        });

        let stdout = stdout();
        let mut stdout = stdout.lock().into_raw_mode()?;

        // Pipe docker exec output into stdout
        while let Some(Ok(output)) = receiver.next().await {
            if let Message::Binary(output) = output {
                stdout.write_all(&output)?;
                stdout.flush()?;
            }
        }

        Ok(())
    }
}
