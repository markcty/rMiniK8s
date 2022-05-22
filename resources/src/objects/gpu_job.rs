use serde::{Deserialize, Serialize};

use super::{pod::PodTemplateSpec, Metadata, Object};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct GpuJob {
    /// Standard object's metadata.
    pub metadata: Metadata,
    /// Specification of the desired behavior of a job.
    pub spec: GpuJobSpec,
    /// Current status of a job.
    pub status: Option<GpuJobStatus>,
}

impl Object for GpuJob {
    fn kind(&self) -> &'static str {
        "GpuJob"
    }

    fn name(&self) -> &String {
        &self.metadata.name
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuJobSpec {
    /// Describes the pod that will be created when executing a job.
    pub template: Option<PodTemplateSpec>,
    /// filename of code zip file
    pub filename: Option<String>,
    /// GPU config of the GpuJob.
    pub gpu_config: GpuConfig,
    /// Specifies the desired number of successfully finished pods the job should be run with.
    /// Setting to nil means that the success of any pod signals the success of all pods,
    /// and allows parallelism to have any positive value.
    /// Setting to 1 means that parallelism is limited to 1 and the success of that pod signals the success of the job.
    #[serde(default = "completions_default")]
    pub completions: u32,
    /// Specifies the maximum desired number of pods the job should run at any given time.
    /// The actual number of pods running in steady state will be less than this number
    /// when ((.spec.completions - .status.successful) < .spec.parallelism)
    #[serde(default = "parallelism_default")]
    pub parallelism: u32,
    /// Specifies the number of retries before marking this job failed. Defaults to 6
    #[serde(default = "back_off_limit_default")]
    pub back_off_limit: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuConfig {
    pub slurm_config: SlurmConfig,
    pub compile_scripts: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlurmConfig {
    pub job_name: String,
    pub partition: String,
    pub total_core_number: u32,
    pub ntasks_per_node: u32,
    pub cpus_per_task: u32,
    pub gres: String,
    pub scripts: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct GpuJobStatus {
    /// The number of pending and running pods.
    pub active: u32,
    /// The number of pods which reached phase Failed.
    pub failed: u32,
    /// The number of pods which reached phase Succeeded.
    pub succeeded: u32,
}

fn completions_default() -> u32 {
    1
}

fn parallelism_default() -> u32 {
    1
}

fn back_off_limit_default() -> u32 {
    6
}
