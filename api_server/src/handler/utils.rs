use std::sync::Arc;

use resources::objects::pod::{Pod, PodPhase, PodStatus};
use serde::Serialize;

use super::response::ErrResponse;
use crate::{etcd, AppState};

pub async fn etcd_put(
    app_state: &Arc<AppState>,
    key: String,
    val: impl Serialize,
) -> Result<(), ErrResponse> {
    let mut client = app_state.get_client().await?;
    etcd::put(&mut client, key, val)
        .await
        .map_err(ErrResponse::from)?;
    Ok(())
}

pub fn update_pod_phase(pod: &mut Pod, new_phase: PodPhase) {
    if let Some(ref mut status) = pod.status {
        status.phase = new_phase;
    } else {
        let new_status: PodStatus = PodStatus {
            phase: new_phase,
            ..PodStatus::default()
        };
        pod.status = Some(new_status);
    }
}
