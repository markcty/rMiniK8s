use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PodSpec {
    /// List of containers belonging to the pod.
    /// Containers cannot currently be added or removed.
    /// There must be at least one container in a Pod. Cannot be updated.
    containers: Vec<Container>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Container {
    /// Name of the container specified as a DNS_LABEL.
    /// Each container in a pod must have a unique name (DNS_LABEL).
    /// Cannot be updated.
    name: String,
    /// Docker image name.
    image: String,
    /// List of ports to expose from the container.
    ports: Vec<ContainerPort>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerPort {
    /// Number of port to expose on the pod's IP address.
    /// This must be a valid port number, 0 < x < 65536.
    #[serde(rename = "containerPort")]
    container_port: u16,
}
