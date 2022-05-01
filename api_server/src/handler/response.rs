use axum::Json;
use resources::models::{ErrResponse, Response};

use crate::etcd::EtcdError;

pub type HandlerResult<T> = Result<Json<Response<T>>, ErrResponse>;

impl From<EtcdError> for ErrResponse {
    fn from(err: EtcdError) -> Self {
        if let Some(cause) = err.cause {
            tracing::debug!("Etcd Error: {}, caused by: {}", err.msg, cause);
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
