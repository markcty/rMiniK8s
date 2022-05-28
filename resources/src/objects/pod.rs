use core::fmt::Write;
use std::{collections::HashMap, default::Default, net::Ipv4Addr};

use bollard::models::{ContainerInspectResponse, ContainerStateStatusEnum, RestartPolicyNameEnum};
use chrono::{Local, NaiveDateTime, TimeZone};
use indenter::indented;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, IntoEnumIterator};

use super::{function::Function, metrics, Metadata, Object};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Pod {
    pub metadata: Metadata,
    pub spec: PodSpec,
    pub status: Option<PodStatus>,
}

impl Object for Pod {
    fn kind(&self) -> &'static str {
        "Pod"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

impl Pod {
    pub fn is_ready(&self) -> bool {
        match &self.status {
            Some(status) => status
                .conditions
                .get(&PodConditionType::Ready)
                .map_or(false, |condition| condition.status),
            None => false,
        }
    }

    pub fn is_active(&self) -> bool {
        if let Some(status) = &self.status {
            matches!(status.phase, PodPhase::Pending | PodPhase::Running)
        } else {
            false
        }
    }

    pub fn requests(&self, resource: &metrics::Resource) -> i64 {
        self.spec
            .containers
            .iter()
            .map(|container| container.requests(resource))
            .sum()
    }

    /// Get a vector of container pairs
    /// (tuple of container spec and container status).
    pub fn container_pairs(&self) -> Vec<ContainerPair> {
        let mut pairs = vec![];
        let statuses = self
            .status
            .as_ref()
            .map_or(vec![], |status| status.container_statuses.clone());
        for container in &self.spec.containers {
            let status = statuses.iter().find(|status| status.name == container.name);
            pairs.push(ContainerPair(container.clone(), status.cloned()));
        }
        pairs
    }
}

impl std::fmt::Display for Pod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:<16} {}", "Name:", self.metadata.name)?;
        if self.status.is_none() {
            return Ok(());
        }
        let status = self.status.as_ref().unwrap();
        writeln!(
            f,
            "{:<16} {}/{}",
            "Node:",
            self.spec
                .node_name
                .as_ref()
                .unwrap_or(&String::from("<none>")),
            status.host_ip.as_ref().unwrap_or(&String::from("<none>"))
        )?;
        writeln!(
            f,
            "{:<16} {}",
            "Start Time:",
            Local.from_utc_datetime(&status.start_time)
        )?;
        writeln!(f, "{:<16} {}", "Labels:", self.metadata.labels.to_string())?;
        writeln!(f, "{:<16} {}", "Phase:", status.phase)?;
        writeln!(
            f,
            "{:<16} {}",
            "IP:",
            status
                .pod_ip
                .as_ref()
                .map_or("<none>".to_string(), |ip| ip.to_string())
        )?;

        writeln!(f, "Containers:")?;
        for pair in self.container_pairs() {
            writeln!(f, "  {}:", pair.0.name)?;
            write!(indented(f), "{}", pair)?;
        }

        writeln!(f, "Conditions:")?;
        writeln!(indented(f), "{:<16} {:<8}", "Type", "Status")?;
        for (condition_type, condition) in status.conditions.iter() {
            writeln!(
                indented(f),
                "{:<16} {:<8}",
                condition_type,
                condition.status
            )?;
        }

        if !self.spec.volumes.is_empty() {
            writeln!(f, "Volumes:")?;
            for volume in &self.spec.volumes {
                writeln!(indented(f), "{}", volume)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodSpec {
    /// List of containers belonging to the pod.
    /// Containers cannot currently be added or removed.
    /// There must be at least one container in a Pod. Cannot be updated.
    pub containers: Vec<Container>,
    /// List of volumes that can be mounted by containers belonging to the pod.
    #[serde(default)]
    pub volumes: Vec<Volume>,
    /// Restart policy for all containers within the pod.
    /// One of Always, OnFailure, Never.
    /// Default to Always.
    #[serde(default)]
    pub restart_policy: RestartPolicy,
    /// Host networking requested for this pod.
    /// Use the host's network namespace.
    /// If this option is set, the ports that will be used must be specified.
    /// Default to false.
    #[serde(default)]
    pub host_network: bool,
    /// NodeName is a request to schedule this pod onto a specific node.
    /// If it is non-empty, the scheduler simply schedules this pod onto that node,
    /// assuming that it fits resource requirements.
    pub node_name: Option<String>,
}

impl PodSpec {
    pub fn network_mode(&self) -> String {
        if self.host_network {
            "host".to_string()
        } else {
            "bridge".to_string()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    /// Name of the container specified as a DNS_LABEL.
    /// Each container in a pod must have a unique name (DNS_LABEL).
    /// Cannot be updated.
    pub name: String,
    /// Docker image name.
    pub image: String,
    /// Image pull policy.
    /// Defaults to Always if :latest tag is specified,
    /// or IfNotPresent otherwise.
    /// Cannot be updated.
    #[serde(default)]
    pub image_pull_policy: Option<ImagePullPolicy>,
    /// Entrypoint array. Not executed within a shell.
    /// The docker image's ENTRYPOINT is used if this is not provided.
    #[serde(default)]
    pub command: Vec<String>,
    /// List of ports to expose from the container.
    pub ports: Vec<ContainerPort>,
    /// Pod volumes to mount into the container's filesystem.
    /// Cannot be updated.
    #[serde(default)]
    pub volume_mounts: Vec<VolumeMount>,
    /// Compute Resources required by this container. Cannot be updated.
    #[serde(default)]
    pub resources: ResourceRequirements,
}

impl Container {
    pub fn requests(&self, resource: &metrics::Resource) -> i64 {
        match resource {
            metrics::Resource::CPU => self.resources.requests.cpu,
            metrics::Resource::Memory => self.resources.requests.memory,
        }
    }

    /// Determine image pull policy.
    ///
    /// Return the image pull policy it's specified.
    /// If the image pull policy is not specified,
    /// fefaults to Always if :latest tag is specified,
    /// or IfNotPresent otherwise.
    ///
    /// # Examples
    /// ```
    /// use resources::objects::pod::{Container, ImagePullPolicy};
    ///
    /// let container = Container {
    ///     name: "nginx".to_string(),
    ///     image: "nginx:latest".to_string(),
    ///     image_pull_policy: Some(ImagePullPolicy::IfNotPresent),
    ///     ..Default::default()
    /// };
    /// assert_eq!(container.image_pull_policy(), ImagePullPolicy::IfNotPresent);
    ///
    /// let container = Container {
    ///     name: "httpd".to_string(),
    ///     image: "httpd:2.4.53".to_string(),
    ///     ..Default::default()
    /// };
    /// assert_eq!(container.image_pull_policy(), ImagePullPolicy::IfNotPresent);
    ///
    /// let container = Container {
    ///     name: "ubuntu".to_string(),
    ///     image: "ubuntu:latest".to_string(),
    ///     ..Default::default()
    /// };
    /// assert_eq!(container.image_pull_policy(), ImagePullPolicy::Always);
    ///
    /// let container = Container {
    ///     name: "debian".to_string(),
    ///     image: "debian:latest".to_string(),
    ///     image_pull_policy: Some(ImagePullPolicy::Never),
    ///     ..Default::default()
    /// };
    /// assert_eq!(container.image_pull_policy(), ImagePullPolicy::Never);
    /// ```
    pub fn image_pull_policy(&self) -> ImagePullPolicy {
        match &self.image_pull_policy {
            Some(policy) => policy.to_owned(),
            None => {
                if self.image.ends_with(":latest") {
                    ImagePullPolicy::Always
                } else {
                    ImagePullPolicy::IfNotPresent
                }
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum ImagePullPolicy {
    Always,
    Never,
    IfNotPresent,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerPort {
    /// Number of port to expose on the pod's IP address.
    /// This must be a valid port number, 0 < x < 65536.
    pub container_port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeMount {
    /// Path within the container at which the volume should be mounted.
    pub mount_path: String,
    /// This must match the Name of a Volume.
    pub name: String,
}

impl std::fmt::Display for VolumeMount {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} from {}", self.mount_path, self.name)
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct ResourceRequirements {
    /// Limits describes the maximum amount of compute resources allowed.
    pub limits: Resource,
    /// Requests describes the minimum amount of compute resources required.
    /// If Requests is omitted for a container,
    /// it defaults to Limits if that is explicitly specified,
    /// otherwise to an implementation-defined value.
    pub requests: Resource,
}

impl std::fmt::Display for ResourceRequirements {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "  Requests:")?;
        write!(indented(f), "{}", self.requests)?;
        writeln!(f, "  Limits:")?;
        write!(indented(f), "{}", self.limits)
    }
}

impl ResourceRequirements {
    pub fn cpu_shares(&self) -> i64 {
        ResourceRequirements::milli_cpu_to_shares(
            if self.requests.cpu == 0 && !self.limits.cpu == 0 {
                self.limits.cpu
            } else {
                self.requests.cpu
            },
        )
    }

    fn milli_cpu_to_shares(milli_cpu: i64) -> i64 {
        const MIN_SHARES: i64 = 2;
        const SHARES_PER_CPU: i64 = 1024;
        const MILLI_CPU_TO_CPU: i64 = 1000;
        if milli_cpu == 0 {
            return MIN_SHARES;
        }
        let shares = (milli_cpu as i64) * SHARES_PER_CPU / MILLI_CPU_TO_CPU;
        shares.max(MIN_SHARES)
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
pub struct Resource {
    /// CPU unit in milli CPU.
    pub cpu: i64,
    /// Memory in bytes. Defaults to 0.
    pub memory: i64,
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "{:<8} {}", "CPU:", self.cpu)?;
        writeln!(f, "{:<8} {}", "Memory:", self.memory)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Volume {
    /// Volume's name.
    /// Must be a DNS_LABEL and unique within the pod.
    pub name: String,
    #[serde(flatten)]
    pub config: VolumeConfig,
}

impl std::fmt::Display for Volume {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "{:<16}: {}", self.name, self.config)
    }
}

#[derive(Debug, Serialize, Deserialize, Display, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum VolumeConfig {
    /// HostPath represents a pre-existing file
    /// or directory on the host machine
    /// that is directly exposed to the container.
    ///
    /// path (String): Path of the directory on the host.
    /// If the path is a symlink, it will follow the link to the real path.
    HostPath(String),
    /// EmptyDir represents a temporary directory that shares a pod's lifetime.
    EmptyDir(()),
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum RestartPolicy {
    Always,
    OnFailure,
    Never,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        RestartPolicy::Always
    }
}

#[allow(clippy::from_over_into)]
impl Into<bollard::models::RestartPolicy> for &RestartPolicy {
    fn into(self) -> bollard::models::RestartPolicy {
        let policy = match self {
            RestartPolicy::Always => RestartPolicyNameEnum::ALWAYS,
            RestartPolicy::OnFailure => RestartPolicyNameEnum::ON_FAILURE,
            RestartPolicy::Never => RestartPolicyNameEnum::NO,
        };

        bollard::models::RestartPolicy {
            name: Some(policy),
            maximum_retry_count: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PodStatus {
    /// IP address of the host to which the pod is assigned.
    /// Empty if not yet scheduled.
    #[serde(rename = "hostIP")]
    pub host_ip: Option<String>,
    /// RFC 3339 date and time at which the object was acknowledged by the Kubelet.
    /// This is before the Kubelet pulled the container image(s) for the pod.
    pub start_time: NaiveDateTime,
    /// The phase of a Pod is a simple, high-level summary
    /// of where the Pod is in its lifecycle.
    pub phase: PodPhase,
    /// IP address allocated to the pod.
    /// Routable at least within the cluster.
    /// Empty if not yet allocated.
    #[serde(rename = "podIP")]
    pub pod_ip: Option<Ipv4Addr>,
    /// Current service state of pod.
    pub conditions: HashMap<PodConditionType, PodCondition>,
    /// The list has one entry per container in the manifest.
    /// Each entry is currently the output of docker inspect.
    pub container_statuses: Vec<ContainerStatus>,
}

impl Default for PodStatus {
    fn default() -> Self {
        let mut conditions = HashMap::new();
        PodConditionType::iter().for_each(|c| {
            conditions.insert(c, PodCondition::default());
        });
        PodStatus {
            host_ip: None,
            start_time: Local::now().naive_utc(),
            phase: PodPhase::Pending,
            pod_ip: None,
            conditions,
            container_statuses: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Display, Eq, PartialEq, Hash)]
pub enum PodPhase {
    /// All containers in the pod have terminated,
    /// and at least one container has terminated in failure.
    /// The container either exited with non-zero status
    /// or was terminated by the system.
    Failed,
    /// The pod has been accepted by the Kubernetes system,
    /// but one or more of the container images has not been created.
    /// This includes time before being scheduled
    /// as well as time spent downloading images over the network,
    /// which could take a while.
    Pending,
    /// The pod has been bound to a node,
    /// and all of the containers have been created.
    /// At least one container is still running,
    /// or is in the process of starting or restarting.
    Running,
    /// All containers in the pod have terminated in success,
    /// and will not be restarted.
    Succeeded,
}

#[derive(Debug, Serialize, Deserialize, Display, PartialEq, Eq, Hash, EnumIter, Clone)]
pub enum PodConditionType {
    /// All containers in the pod are ready.
    ContainersReady,
    /// All init containers have completed successfully.
    Initialized,
    /// The pod has been scheduled to a node.
    PodScheduled,
    /// The pod is able to serve requests
    /// and should be added to the load balancing pools of all matching Services.
    Ready,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, Eq, PartialEq)]
pub struct PodCondition {
    /// Status is the status of the condition.
    /// Can be True, False, Unknown.
    pub status: bool,
}

#[derive(Debug, Serialize, Deserialize, Display, Clone, Eq, PartialEq)]
pub enum ContainerState {
    Running,
    Terminated { exit_code: i64 },
    Waiting,
}

impl From<Option<bollard::models::ContainerState>> for ContainerState {
    fn from(state: Option<bollard::models::ContainerState>) -> Self {
        match state {
            Some(state) => match state.status {
                Some(status) => match status {
                    ContainerStateStatusEnum::RUNNING => ContainerState::Running,
                    ContainerStateStatusEnum::EXITED | ContainerStateStatusEnum::DEAD => {
                        ContainerState::Terminated {
                            exit_code: state.exit_code.unwrap_or(0),
                        }
                    },
                    _ => ContainerState::Waiting,
                },
                None => ContainerState::Waiting,
            },
            None => ContainerState::Waiting,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatus {
    /// This must be a DNS_LABEL.
    /// Each container in a pod must have a unique name.
    /// Cannot be updated.
    pub name: String,
    /// The image the container is running.
    pub image: String,
    /// Container's ID
    pub container_id: String,
    /// State is the current state of the container.
    pub state: ContainerState,
    /// The number of times the container has been restarted.
    pub restart_count: u32,
}

impl From<ContainerInspectResponse> for ContainerStatus {
    fn from(response: ContainerInspectResponse) -> Self {
        let labels = response.config.and_then(|c| c.labels).unwrap_or_default();
        let name = labels
            .get("minik8s.container.name")
            .unwrap_or(&"".to_string())
            .to_owned();
        ContainerStatus {
            name,
            image: response.image.expect("Container image not found"),
            container_id: response.id.expect("Container ID not found"),
            state: ContainerState::from(response.state),
            restart_count: response.restart_count.unwrap_or(0) as u32,
        }
    }
}

impl Pod {
    pub fn get_ip(&self) -> Option<Ipv4Addr> {
        if let Some(status) = &self.status {
            if let Some(ip) = &status.pod_ip {
                return Some(ip.to_owned());
            }
        }
        None
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PodTemplateSpec {
    /// Standard object's metadata.
    pub metadata: Metadata,
    /// Specification of the desired behavior of the pod.
    pub spec: PodSpec,
}

pub struct ContainerPair(Container, Option<ContainerStatus>);

impl std::fmt::Display for ContainerPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (container, status) = (&self.0, &self.1);
        writeln!(f, "{:<16} {}", "Image:", container.image)?;
        writeln!(
            f,
            "{:<16} {}",
            "Port:",
            container
                .ports
                .iter()
                .map(|port| port.container_port.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )?;
        if let Some(status) = status {
            writeln!(f, "{:<16} {}", "Container ID:", status.container_id)?;
            writeln!(f, "{:<16} {}", "State:", status.state)?;
            writeln!(f, "{:<16} {}", "Restart Count:", status.restart_count)?;
        }
        if !container.volume_mounts.is_empty() {
            writeln!(f, "Mounts")?;
            for volume_mount in &container.volume_mounts {
                writeln!(indented(f), "{}", volume_mount)?;
            }
        }
        writeln!(f, "Resources:")?;
        write!(f, "{}", container.resources)
    }
}

impl PodTemplateSpec {
    pub fn from_function(func: &Function) -> Self {
        let func_name = func.metadata.name.to_owned();
        let metadata = Metadata {
            name: func_name.to_owned(),
            uid: None,
            labels: func.metadata.labels.clone(),
            ..Default::default()
        };
        let spec = PodSpec {
            containers: vec![Container {
                name: func_name,
                image: func
                    .status
                    .as_ref()
                    .unwrap()
                    .image
                    .as_ref()
                    .unwrap()
                    .to_owned(),
                image_pull_policy: Some(ImagePullPolicy::IfNotPresent),
                ports: vec![ContainerPort {
                    container_port: 80,
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        Self {
            metadata,
            spec,
        }
    }
}
