use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Multipart, Query},
    Extension, Json,
};
use axum_macros::debug_handler;
use resources::models::{ErrResponse, Response};

use super::{response::HandlerResult, utils::*};
use crate::AppState;

#[debug_handler]
pub async fn create(
    Extension(_app_state): Extension<Arc<AppState>>,
    Query(param): Query<HashMap<String, String>>,
    mut multipart: Multipart,
) -> HandlerResult<()> {
    let name = if let Some(name) = param.get("name") {
        name
    } else {
        let err = ErrResponse::bad_request("function name should be specified".to_string(), None);
        return Err(err);
    };

    let field = if let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ErrResponse::new(e.to_string(), None))?
    {
        field
    } else {
        let err = ErrResponse::bad_request("File upload failed".to_string(), None);
        return Err(err);
    };

    let original_filename = field.file_name().map_or("".to_string(), |n| n.to_owned());
    if !original_filename.ends_with(".zip") {
        let err = ErrResponse::bad_request("Please upload a zip file".to_string(), None);
        return Err(err);
    }

    let _filename = stream_to_tmp_file(original_filename.as_str(), field).await?;

    let res = Response::new(Some(format!("function/{} created", name)), None);
    Ok(Json(res))
}
