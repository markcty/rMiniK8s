use axum::{http::StatusCode, response::IntoResponse, Json};
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
        }
    }
}

impl IntoResponse for ErrResponse {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}
