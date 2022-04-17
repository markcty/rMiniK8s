use bollard::Docker;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref DOCKER: Docker =
        Docker::connect_with_local_defaults().expect("Failed to connect to docker daemon");
}

pub const PAUSE_IMAGE_NAME: &str = "docker/desktop-kubernetes-pause:3.5";
pub const CONTAINER_NAME_PREFIX: &str = "minik8s";
pub const PAUSE_CONTAINER_NAME: &str = "POD";
