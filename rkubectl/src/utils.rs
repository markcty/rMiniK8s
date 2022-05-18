use anyhow::Result;
use chrono::{Local, NaiveDateTime};
use chrono_humanize::{Accuracy, HumanTime, Tense};
use resources::objects::Object;

use crate::{objects::KubeObject, Url, CONFIG};

pub fn gen_url_from_object(object: &KubeObject) -> Result<Url> {
    let url = CONFIG.base_url.to_owned();
    let uri = object.uri();
    Ok(url.join(uri.as_str())?)
}

pub fn gen_url(kind: String, name: Option<&String>) -> Result<Url> {
    let url = CONFIG.base_url.to_owned();
    let path = if let Some(name) = name {
        format!("api/v1/{}/{}", kind, name)
    } else {
        format!("api/v1/{}", kind)
    };
    Ok(url.join(path.as_str())?)
}

pub fn calc_age(time: NaiveDateTime) -> String {
    let d = HumanTime::from(Local::now().naive_utc() - time);
    d.to_text_en(Accuracy::Rough, Tense::Present)
}
