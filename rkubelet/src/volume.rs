use std::{fs::create_dir_all, path::PathBuf};

use anyhow::{Context, Result};
use resources::objects::{pod, pod::VolumeConfig};

#[derive(Debug)]
pub struct Volume {
    pod_dir: PathBuf,
    volume: pod::Volume,
}

impl Volume {
    pub fn new(pod_dir: PathBuf, volume: pod::Volume) -> Self {
        Self {
            pod_dir,
            volume,
        }
    }

    pub fn create(pod_dir: PathBuf, volume: pod::Volume) -> Result<Self> {
        let volume = Volume::new(pod_dir, volume);
        volume.provision()?;
        Ok(volume)
    }

    fn provision(&self) -> Result<()> {
        match &self.volume.config {
            VolumeConfig::EmptyDir(_) => self.provision_empty_dir(),
            VolumeConfig::HostPath(path) => self.provision_host_path(path),
        }
    }

    fn provision_host_path(&self, _path: &str) -> Result<()> {
        Ok(())
    }

    fn provision_empty_dir(&self) -> Result<()> {
        create_dir_all(self.host_src())
            .with_context(|| format!("Failed to create empty dir volume {}", self.volume.name))
    }

    pub fn host_src(&self) -> String {
        match &self.volume.config {
            VolumeConfig::EmptyDir(_) => {
                let mut path = PathBuf::from(&self.pod_dir);
                path.push("volumes");
                path.push(&self.volume.name);
                path.to_str().unwrap().to_string()
            },
            VolumeConfig::HostPath(path) => path.to_owned(),
        }
    }
}
