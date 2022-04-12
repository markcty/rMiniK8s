use anyhow::{Context, Result};
use etcd_client::Client;

use crate::SERVER_CONFIG;

pub async fn connect_etcd() -> Result<etcd_client::Client> {
    let client = Client::connect([SERVER_CONFIG.etcd_url.to_owned()], None)
        .await
        .with_context(|| format!("Failed to connect to etcd"))?;
    println!("Successfully connect to etcd");
    Ok(client)
}
