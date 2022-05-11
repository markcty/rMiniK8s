use std::{collections::HashMap, error::Error, net::Ipv4Addr, rc::Rc};

use anyhow::{anyhow, Context, Result};
use iptables::IPTables;
use rand::distributions::{Alphanumeric, DistString};
use resources::objects::{service::ServicePort, KubeObject, KubeResource::Service};

pub struct K8sIpTables {
    ipt: Rc<IPTables>,
    // map service name to service table
    svc_tb: HashMap<String, ServiceTable>,
}

struct ServiceTable {
    ipt: Rc<IPTables>,
    svc_name: String,
    cluster_ip: Ipv4Addr,
    // every port has a load balance table
    lb_tb: HashMap<u16, LBTable>,
}

impl ServiceTable {
    fn new(ipt: Rc<IPTables>, svc_name: String, cluster_ip: Ipv4Addr) -> Self {
        Self {
            ipt,
            svc_name,
            cluster_ip,
            lb_tb: HashMap::new(),
        }
    }

    fn add_ep(&mut self, ep: &Ipv4Addr) {
        for (_, lb) in self.lb_tb.iter_mut() {
            lb.add_ep(ep);
        }

        tracing::info!("Add endpoint {} for service {}", ep, self.svc_name);
    }

    fn cleanup(self) {
        let svc_name = self.svc_name;
        let cluster_ip = self.cluster_ip;
        for (port, lb) in self.lb_tb {
            let rule = Self::gen_svc_rule(&svc_name, &cluster_ip, &port, &lb.lb_chain);
            self.ipt
                .delete("nat", "KUBE-SERVICES", rule.as_str())
                .unwrap();
            lb.cleanup();
        }
    }

    fn add_port(&mut self, port: u16, target_port: u16) {
        let lb_tb = LBTable::new(self.ipt.clone(), self.svc_name.to_owned(), target_port);

        let rule = Self::gen_svc_rule(&self.svc_name, &self.cluster_ip, &port, &lb_tb.lb_chain);
        self.ipt
            .append("nat", "KUBE-SERVICES", rule.as_str())
            .unwrap();

        self.lb_tb.insert(port, lb_tb);

        tracing::info!(
            "Add port {}, target port {} for service {}",
            port,
            target_port,
            self.svc_name
        );
    }

    fn gen_svc_rule(
        svc_name: &String,
        cluster_ip: &Ipv4Addr,
        port: &u16,
        lb_chain: &String,
    ) -> String {
        format!(
            "-d {} -p tcp --dport {} -m comment --comment \"{}\" -j {}",
            cluster_ip, port, svc_name, lb_chain
        )
    }

    fn change_cluster_ip(&mut self, new_cluster_ip: &Ipv4Addr) {
        let old_cluster_ip = self.cluster_ip;
        self.cluster_ip = new_cluster_ip.to_owned();

        for (port, lb) in self.lb_tb.iter() {
            let old_rule = Self::gen_svc_rule(&self.svc_name, &old_cluster_ip, port, &lb.lb_chain);
            self.ipt
                .delete("nat", "KUBE-SERVICES", old_rule.as_str())
                .unwrap();

            let new_rule = Self::gen_svc_rule(&self.svc_name, new_cluster_ip, port, &lb.lb_chain);
            self.ipt
                .append("nat", "KUBE-SERVICES", new_rule.as_str())
                .unwrap();
        }
    }
}

struct LBTable {
    ipt: Rc<IPTables>,
    svc_name: String,
    lb_chain: String,
    target_port: u16,
    // endpoint ip, endpoint chain
    eps: HashMap<Ipv4Addr, String>,
}

impl LBTable {
    pub fn new(ipt: Rc<IPTables>, svc_name: String, target_port: u16) -> Self {
        let lb_chain = format!("KUBE-LB-{}", unique_hash());
        ipt.new_chain("nat", lb_chain.as_str())
            .expect("IpTable inconsistent, load balance chain exists");
        Self {
            ipt,
            svc_name,
            lb_chain,
            target_port,
            eps: HashMap::new(),
        }
    }

    fn add_ep(&mut self, ep: &Ipv4Addr) {
        // new endpoint chain
        let ep_chain = format!("KUBE-EP-{}", unique_hash());
        self.ipt.new_chain("nat", ep_chain.as_str()).unwrap();
        let rule = format!(
            "-p tcp -m comment --comment \"{}\" -m tcp -j DNAT --to-destination {}:{}",
            self.svc_name, ep, self.target_port
        );
        self.ipt
            .append_unique("nat", ep_chain.as_str(), rule.as_str())
            .unwrap();
        self.eps.insert(ep.to_owned(), ep_chain);

        // rewrite lb rules
        self.rewrite_lb_rulus();
    }

    fn rewrite_lb_rulus(&self) {
        self.ipt.flush_chain("nat", self.lb_chain.as_str()).unwrap();
        let mut i = 1;
        for (_, ep_chain) in self.eps.iter() {
            let rule = Self::gen_lb_rule(i, ep_chain);
            self.ipt
                .insert("nat", self.lb_chain.as_str(), rule.as_str(), 1)
                .unwrap();
            i += 1;
        }
    }

    fn gen_lb_rule(i: i32, ep_chain: &String) -> String {
        if i == 1 {
            format!("-j {}", ep_chain)
        } else {
            format!(
                "-m statistic --mode random --probability {:.11} -j {}",
                1.0 / i as f32,
                ep_chain
            )
        }
    }

    fn cleanup(self) {
        self.ipt.flush_chain("nat", self.lb_chain.as_str()).unwrap();
        self.ipt
            .delete_chain("nat", self.lb_chain.as_str())
            .unwrap();
        for (_, ep_chain) in self.eps {
            self.ipt.flush_chain("nat", &ep_chain).unwrap();
            self.ipt.delete_chain("nat", ep_chain.as_str()).unwrap();
        }
    }
}

fn unique_hash() -> String {
    Alphanumeric
        .sample_string(&mut rand::thread_rng(), 8)
        .to_uppercase()
}

impl K8sIpTables {
    pub fn new() -> Self {
        let ipt = iptables::new(false)
            .expect("Failed to open iptables, please ensure you are in root mode");

        let ipt = Self {
            ipt: Rc::new(ipt),
            svc_tb: HashMap::new(),
        };

        ipt.cleanup()
            .expect("Failed to open iptables, please ensure you are in root mode");
        ipt
    }

    fn cleanup(&self) -> Result<(), Box<dyn Error>> {
        let chains = &self.ipt.list_chains("nat")?;
        if !chains.contains(&"KUBE-SERVICES".to_string()) {
            self.ipt.new_chain("nat", "KUBE-SERVICES")?;
        }

        if !self.ipt.exists(
            "nat",
            "PREROUTING",
            r#"-m comment --comment "kubernetes service portals" -j KUBE-SERVICES"#,
        )? {
            self.ipt.insert(
                "nat",
                "PREROUTING",
                r#"-m comment --comment "kubernetes service portals" -j KUBE-SERVICES"#,
                1,
            )?;
        }

        if !self.ipt.exists(
            "nat",
            "OUTPUT",
            r#"-m comment --comment "kubernetes service portals" -j KUBE-SERVICES"#,
        )? {
            self.ipt.insert(
                "nat",
                "OUTPUT",
                r#"-m comment --comment "kubernetes service portals" -j KUBE-SERVICES"#,
                1,
            )?;
        }

        // remove entrypoint rules
        if chains.contains(&"KUBE-SERVICES".to_string()) {
            self.ipt.flush_chain("nat", "KUBE-SERVICES")?;
        }

        // remove load balancing rules
        for chain in chains.iter() {
            if chain.starts_with("KUBE-LB") {
                self.ipt.flush_chain("nat", chain.as_str())?;
                self.ipt.delete_chain("nat", chain.as_str())?;
            }
        }

        // remove endpoint rules
        for chain in chains.iter() {
            if chain.starts_with("KUBE-EP") {
                self.ipt.flush_chain("nat", chain.as_str())?;
                self.ipt.delete_chain("nat", chain.as_str())?;
            }
        }

        Ok(())
    }

    pub fn new_svc(&mut self, name: &String, cluster_ip: &Ipv4Addr) {
        if self.svc_tb.contains_key(name) {
            tracing::warn!("IpTable inconsistent, service {} exists", name);
            return;
        }

        let svc_table = ServiceTable::new(self.ipt.clone(), name.to_owned(), cluster_ip.to_owned());
        self.svc_tb.insert(name.to_owned(), svc_table);
    }

    fn add_svc_port(&mut self, name: &String, port: u16, target_port: u16) {
        let svc_tb = if let Some(tb) = self.svc_tb.get_mut(name) {
            tb
        } else {
            tracing::warn!("IpTable inconsistent, service {} doesn't exist", name);
            return;
        };

        svc_tb.add_port(port, target_port);
    }

    fn add_svc_ep(&mut self, name: &String, ep: &Ipv4Addr) {
        let svc_table = if let Some(svc_tb) = self.svc_tb.get_mut(name) {
            svc_tb
        } else {
            tracing::warn!("IpTable inconsistent, service {} doesn't exist", name);
            return;
        };

        svc_table.add_ep(ep);
    }

    #[allow(dead_code)]
    fn change_cluster_ip(&mut self, name: &String, new_cluster_ip: &Ipv4Addr) {
        let svc_table = if let Some(svc_tb) = self.svc_tb.get_mut(name) {
            svc_tb
        } else {
            tracing::warn!("IpTable inconsistent, service {} doesn't exist", name);
            return;
        };

        svc_table.change_cluster_ip(new_cluster_ip);
    }

    pub fn add_svc(&mut self, svc: &KubeObject) {
        let name = svc.name();
        let cluster_ip = get_svc_cluster_ip(svc).expect("Service should always has a cluster ip");
        self.new_svc(&name, &cluster_ip);

        let svc_ports = get_svc_ports(svc);
        for port in svc_ports {
            self.add_svc_port(&name, port.port, port.target_port);
        }

        let eps = get_svc_eps(svc);
        for ep in eps {
            self.add_svc_ep(&name, &ep);
        }
    }

    pub fn del_svc(&mut self, name: &String) {
        let svc_tb = if let Some(tb) = self.svc_tb.remove(name) {
            tb
        } else {
            tracing::warn!("IpTable inconsistent, service {} doesn't exist", name);
            return;
        };

        svc_tb.cleanup();

        tracing::info!("Delete service {}", name);
    }
}

fn get_svc_eps(svc: &KubeObject) -> Vec<Ipv4Addr> {
    if let Service(ref svc) = svc.resource {
        svc.spec.endpoints.clone()
    } else {
        vec![]
    }
}

fn get_svc_ports(svc: &KubeObject) -> Vec<ServicePort> {
    if let Service(ref svc) = svc.resource {
        svc.spec.ports.clone()
    } else {
        vec![]
    }
}

fn get_svc_cluster_ip(svc: &KubeObject) -> Result<Ipv4Addr> {
    if let Service(ref svc) = svc.resource {
        let ip = svc
            .spec
            .cluster_ip
            .context("Service has no cluster ip, should not happen")?;
        Ok(ip)
    } else {
        Err(anyhow!("Not servcie"))
    }
}

#[cfg(test)]
mod test {
    use std::net::Ipv4Addr;

    use super::K8sIpTables;

    #[test]
    fn svc() {
        let mut ipt = K8sIpTables::new();

        let name = "test-service".to_string();
        let cluster_ip = Ipv4Addr::new(172, 16, 0, 1);

        ipt.new_svc(&name, &cluster_ip);
        ipt.add_svc_port(&name, 80, 80);

        let ep1 = Ipv4Addr::new(10, 5, 28, 2);
        let ep2 = Ipv4Addr::new(10, 5, 28, 3);

        ipt.add_svc_ep(&name, &ep1);
        ipt.add_svc_ep(&name, &ep2);

        ipt.del_svc(&name);
    }
}
