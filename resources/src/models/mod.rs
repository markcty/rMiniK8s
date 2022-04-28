use serde::{Deserialize, Serialize};

pub mod etcd;

#[derive(Debug, Serialize, Deserialize)]
pub struct Response<T: Serialize> {
    pub msg: Option<String>,
    pub data: Option<T>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrResponse {
    pub msg: String,
    pub cause: Option<String>,
}
