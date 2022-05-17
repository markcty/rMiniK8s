use std::net::Ipv4Addr;

use nginx_ingress_config::{IngressHost, NginxIngressConfig};

mod nginx_ingress_config;

use anyhow::Result;

fn main() -> Result<()> {
    let mut config = NginxIngressConfig::new();

    let mut host = IngressHost::new("nginx.minik8s.com".to_string());
    host.add_path("/".to_string(), Ipv4Addr::new(10, 5, 28, 2), 80);
    config.add_host(host);

    let mut host = IngressHost::new("tomcat.minik8s.com".to_string());
    host.add_path("/".to_string(), Ipv4Addr::new(10, 5, 28, 3), 80);
    config.add_host(host);

    println!("{}", config.to_string());
    config.flush();

    Ok(())
}
