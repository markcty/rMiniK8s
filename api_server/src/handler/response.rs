use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

use crate::etcd::EtcdError;

#[derive(Debug, Serialize)]
pub struct Response<T: Serialize> {
    pub msg: String,
    pub data: Option<T>,
}

#[derive(Debug, Serialize)]
pub struct ErrResponse {
    pub msg: String,
    pub cause: Option<String>,
}

pub type HandlerResult<T> = Result<Json<Response<T>>, ErrResponse>;

impl<T> Response<T>
where
    T: Serialize,
{
    pub fn new(msg: String, data: Option<T>) -> Self {
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

impl From<EtcdError> for ErrResponse {
    fn from(err: EtcdError) -> Self {
        if let Some(cause) = err.cause {
            tracing::debug!("Etcd Error: {}\nCaused by: \n{}", err.msg, cause);
        } else {
            tracing::debug!("Etcd Error: {}", err.msg);
        }
        Self {
            msg: "Etcd Error".to_string(),
            // the error of database should not be forwarded to client
            cause: None,
        }
    }
}

impl IntoResponse for ErrResponse {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}
