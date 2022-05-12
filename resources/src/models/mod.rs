use axum::{http::StatusCode, response::IntoResponse, Json};
use reqwest::Url;
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
    #[serde(skip)]
    pub status: StatusCode,
}

pub struct NodeConfig {
    pub etcd_endpoint: Url,
    pub api_server_endpoint: Url,
}

impl<T> Response<T>
where
    T: Serialize,
{
    pub fn new(msg: Option<String>, data: Option<T>) -> Self {
        Self {
            msg,
            data,
        }
    }
}

impl ErrResponse {
    pub fn new(msg: String, cause: Option<String>) -> Self {
        Self {
            msg,
            cause,
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
    pub fn not_found(msg: String, cause: Option<String>) -> Self {
        Self {
            msg,
            cause,
            status: StatusCode::NOT_FOUND,
        }
    }
}

impl IntoResponse for ErrResponse {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self)).into_response()
    }
}
