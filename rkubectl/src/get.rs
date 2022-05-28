use std::vec::Vec;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use reqwest::blocking::Client;
use resources::{
    models::Response,
    objects::{
        node::NodeAddressType,
        KubeObject::{
            self, Function, GpuJob, HorizontalPodAutoscaler, Ingress, Node, Pod, ReplicaSet,
            Service,
        },
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
        let data = if self.name.is_none() {
            let res = client
                .get(url)
                .send()?
                .json::<Response<Vec<KubeObject>>>()?;
            res.data.unwrap_or_default()
        } else {
            let res = client.get(url).send()?.json::<Response<KubeObject>>()?;
            res.data.map_or_else(Vec::new, |data| vec![data])
        };

        match self.kind {
            ResourceKind::Pods => {
                println!(
                    "{:<20} {:<10} {:<8} {:<10}",
                    "NAME", "STATUS", "RESTARTS", "AGE"
                );
                for object in data {
                    if let Pod(pod) = object {
                        let status = pod.status.as_ref().unwrap();
                        let restarts = status
                            .container_statuses
                            .iter()
                            .map(|c| c.restart_count)
                            .sum::<u32>();
                        println!(
                            "{:<20} {:<10} {:<8} {:<10}",
                            pod.metadata.name,
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
                for object in data {
                    if let ReplicaSet(rs) = object {
                        let status = rs.status.unwrap_or_default();
                        println!(
                            "{:<20} {:<8} {:<8} {:<8}",
                            rs.metadata.name,
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
                for object in data {
                    if let Service(svc) = object {
                        let ports = svc
                            .spec
                            .ports
                            .iter()
                            .map(|port| {
                                if port.port == port.target_port {
                                    port.port.to_string()
                                } else {
                                    format!("{}:{},", port.port, port.target_port)
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(",");
                        let eps = svc
                            .spec
                            .endpoints
                            .iter()
                            .map(|ip| ip.to_string())
                            .collect::<Vec<_>>()
                            .join(",");
                        println!(
                            "{: <20} {: <16} {: <20} {:<}",
                            svc.metadata.name,
                            svc.spec.cluster_ip.ok_or_else(|| anyhow!(
                                "Service should always have a cluster IP"
                            ))?,
                            ports,
                            eps
                        );
                    }
                }
            },
            ResourceKind::Ingresses => {
                println!("{:<20} {:<30} PATH:SERVICE:PORT", "NAME", "HOST");
                for object in data {
                    if let Ingress(ingress) = object {
                        let name = ingress.metadata.name;
                        for rule in ingress.spec.rules {
                            let paths = rule
                                .paths
                                .iter()
                                .map(|path| {
                                    format!(
                                        "{}:{}:{}",
                                        path.path, path.service.name, path.service.port
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(",");

                            println!("{:<20} {:<30} {}", name, rule.host.unwrap(), paths);
                        }
                    }
                }
            },
            ResourceKind::HorizontalPodAutoscalers => {
                println!(
                    "{:<16} {:<24} {:<8} {:<8} {:<}",
                    "NAME", "REFERENCE", "CURRENT", "DESIRED", "LAST SCALE"
                );
                for object in data {
                    if let HorizontalPodAutoscaler(hpa) = object {
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
                            hpa.metadata.name,
                            reference,
                            status.current_replicas,
                            status.desired_replicas,
                            last_scale
                        );
                    }
                }
            },
            ResourceKind::GpuJobs => {
                println!(
                    "{:<20} {:<10} {:<8} {:<8} {:<8}",
                    "NAME", "DESIRED", "ACTIVE", "FAILED", "SUCCEEDED"
                );
                for object in data {
                    if let GpuJob(job) = object {
                        let status = job.status.unwrap_or_default();
                        println!(
                            "{:<20} {:<10} {:<8} {:<8} {:<8}",
                            job.metadata.name,
                            job.spec.completions,
                            status.active,
                            status.failed,
                            status.succeeded
                        );
                    }
                }
            },
            ResourceKind::Nodes => {
                println!(
                    "{:<16} {:<16} {:<}",
                    "NAME", "LAST HEARTBEAT", "INTERNAL IP"
                );
                for object in data {
                    if let Node(node) = object {
                        println!(
                            "{:<16} {:<16} {:<}",
                            node.metadata.name,
                            calc_age(node.status.last_heartbeat),
                            node.status
                                .addresses
                                .get(&NodeAddressType::InternalIP)
                                .unwrap_or(&"".to_string())
                        );
                    }
                }
            },
            ResourceKind::Functions => {
                println!(
                    "{:<16} {:<10} {:<8} {:<8} {:<8}",
                    "NAME", "STATUS", "CURRENT", "DESIRED", "LAST SCALE"
                );
                for object in data {
                    if let Function(func) = object {
                        let status = func.status.expect("Function has no status");

                        // Query HPA
                        let url = gen_url(
                            "horizontalpodautoscalers".to_string(),
                            Some(&func.metadata.name),
                        )?;
                        let res = client.get(url).send()?.json::<Response<KubeObject>>()?;
                        let hpa_status = if let HorizontalPodAutoscaler(hpa) =
                            res.data.expect("Failed to get HPA")
                        {
                            hpa.status.expect("HPA has no status")
                        } else {
                            continue;
                        };

                        let state = if status.image.is_none() {
                            "Pending"
                        } else if hpa_status.current_replicas > 0 {
                            "Ready"
                        } else {
                            "Hibernated"
                        };

                        println!(
                            "{:<16} {:<10} {:<8} {:<8} {:<8}",
                            func.metadata.name,
                            state,
                            hpa_status.current_replicas,
                            hpa_status.desired_replicas,
                            hpa_status
                                .last_scale_time
                                .map(calc_age)
                                .unwrap_or_else(|| "Never".to_string())
                        );
                    }
                }
            },
        }

        Ok(())
    }
}
