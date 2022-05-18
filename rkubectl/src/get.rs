use anyhow::{anyhow, Context, Result};
use clap::Args;
use reqwest::blocking::Client;
use resources::{
    models::Response,
    objects::{
        KubeObject,
        KubeResource::{HorizontalPodAutoscaler, Pod, ReplicaSet, Service},
    },
};

use crate::{
    utils::{calc_age, gen_url},
    ResourceKind,
};

#[derive(Args)]
pub struct Arg {
    /// Kind of resource
    #[clap(arg_enum)]
    kind: ResourceKind,
    /// Name of resource
    name: Option<String>,
}

impl Arg {
    pub fn handle(&self) -> Result<()> {
        let client = Client::new();
        let url = gen_url(self.kind.to_string(), self.name.as_ref())?;
        let res = client
            .get(url)
            .send()?
            .json::<Response<Vec<KubeObject>>>()?;

        match self.kind {
            ResourceKind::Pods => {
                println!(
                    "{:<20} {:<10} {:<8} {:<10}",
                    "NAME", "STATUS", "RESTARTS", "AGE"
                );
                for object in res.data.unwrap() {
                    if let Pod(pod) = object.resource {
                        let status = pod.status.as_ref().unwrap();
                        let restarts = status
                            .container_statuses
                            .iter()
                            .map(|c| c.restart_count)
                            .sum::<u32>();
                        println!(
                            "{:<20} {:<10} {:<8} {:<10}",
                            object.metadata.name,
                            status.phase,
                            restarts,
                            calc_age(status.start_time)
                        );
                    }
                }
            },
            ResourceKind::ReplicaSets => {
                println!(
                    "{:<20} {:<8} {:<8} {:<8}",
                    "NAME", "DESIRED", "CURRENT", "READY"
                );
                for object in res.data.unwrap() {
                    if let ReplicaSet(rs) = object.resource {
                        let status = rs.status.unwrap_or_default();
                        println!(
                            "{:<20} {:<8} {:<8} {:<8}",
                            object.metadata.name,
                            rs.spec.replicas,
                            status.replicas,
                            status.ready_replicas,
                        );
                    }
                }
            },
            ResourceKind::Services => {
                println!(
                    "{:<20} {:<16} {:<20} {:<}",
                    "NAME", "CLUSTER-IP", "PORTS", "ENDPOINTS"
                );
                for object in res.data.unwrap() {
                    if let Service(svc) = object.resource {
                        let mut ports = svc.spec.ports.iter().fold("".to_string(), |sum, port| {
                            if port.port == port.target_port {
                                sum + &port.port.to_string() + ","
                            } else {
                                sum + &format!("{}:{},", port.port, port.target_port)
                            }
                        });
                        ports.pop();
                        let mut eps = svc
                            .spec
                            .endpoints
                            .iter()
                            .fold("".to_string(), |sum, ip| sum + &ip.to_string() + ",");
                        eps.pop();
                        println!(
                            "{: <20} {: <16} {: <20} {:<}",
                            object.metadata.name,
                            svc.spec.cluster_ip.ok_or_else(|| anyhow!(
                                "Service should always have a cluster IP"
                            ))?,
                            ports,
                            eps
                        );
                    }
                }
            },
            ResourceKind::HorizontalPodAutoscalers => {
                println!(
                    "{:<16} {:<24} {:<8} {:<8} {:<}",
                    "NAME", "REFERENCE", "CURRENT", "DESIRED", "LAST SCALE"
                );
                for object in res.data.unwrap() {
                    if let HorizontalPodAutoscaler(hpa) = object.resource {
                        let status = hpa
                            .status
                            .with_context(|| anyhow!("HorizontalPodAutoscaler has no status"))?;
                        let last_scale = status
                            .last_scale_time
                            .map(calc_age)
                            .unwrap_or_else(|| "Never".to_string());
                        let scale_target = hpa.spec.scale_target_ref;
                        let reference = format!("{}/{}", scale_target.kind, scale_target.name);
                        println!(
                            "{:<16} {:<24} {:<8} {:<8} {:<}",
                            object.metadata.name,
                            reference,
                            status.current_replicas,
                            status.desired_replicas,
                            last_scale
                        );
                    }
                }
            },
        }

        Ok(())
    }
}
