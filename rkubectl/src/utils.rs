use anyhow::Result;

use crate::{objects::KubeObject, Url, CONFIG};

pub fn gen_url_from_object(object: &KubeObject) -> Result<Url> {
    let url = CONFIG.base_url.to_owned();
    let path = format!("{}s/{}", object.kind(), object.metadata.name.to_lowercase());
    Ok(url.join(path.as_str())?)
}

pub fn gen_url(kind: &str, name: &str) -> Result<Url> {
    let url = CONFIG.base_url.to_owned();
    let path = format!("{}s/{}", kind, name.to_lowercase());
    Ok(url.join(path.as_str())?)
}
