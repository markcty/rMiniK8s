use anyhow::Context;
use bollard::Docker;
use config::{Config, File};
use lazy_static::lazy_static;
use resources::config::kubelet::KubeletConfig;

lazy_static! {
    pub static ref DOCKER: Docker =
        Docker::connect_with_local_defaults().expect("Failed to connect to docker daemon");
    pub static ref CONFIG: KubeletConfig = Config::builder()
        .add_source(File::with_name("/var/lib/rkubelet/config.yaml"))
        .build()
        .unwrap_or_default()
        .try_deserialize::<KubeletConfig>()
        .with_context(|| "Failed to parse config".to_string())
        .unwrap_or_default();
}

pub const PAUSE_IMAGE_NAME: &str = "docker/desktop-kubernetes-pause:3.5";
pub const CONTAINER_NAME_PREFIX: &str = "minik8s";
pub const PAUSE_CONTAINER_NAME: &str = "POD";
pub const POD_DIR_PATH: &str = "/var/lib/rkubelet/pods";
