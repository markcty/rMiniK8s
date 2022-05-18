use std::{fs::OpenOptions, io::Write, net::Ipv4Addr, process::Command};

const CONFIG_HEADER: &str = r#"
user www-data;
worker_processes auto;
pid /run/nginx.pid;
include /etc/nginx/modules-enabled/*.conf;

events {
	worker_connections 768;
}

"#;

const CONFIG_BASE: &str = r#"
http {

    SERVERS

    server {
        listen       80 default_server;
        server_name  _;
    
        # Everything is a 404
        location / {
            return 404;
        }
    }
}
"#;

const SERVER_BASE: &str = r#"
server {

server_name SERVER_NAME;
listen 80;

LOCATIONS

}
"#;

const LOCATION_BASE: &str = r#"
location PATH {
    proxy_pass http://IP:PORT/;
}
"#;

pub struct NginxIngressConfig {
    servers: Vec<IngressHost>,
}

pub struct IngressHost {
    host: String,
    paths: Vec<IngressPath>,
}

pub struct IngressPath {
    path: String,
    ip: Ipv4Addr,
    port: u16,
}

impl NginxIngressConfig {
    pub fn new() -> Self {
        Self {
            servers: vec![],
        }
    }

    pub fn add_host(&mut self, host: IngressHost) {
        self.servers.push(host);
    }

    pub fn flush(&self) {
        let config = self.to_string();
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open("/etc/nginx/nginx.conf")
            .unwrap();
        file.write_all(config.as_bytes()).unwrap();
        Command::new("nginx")
            .args(["-s", "reload"])
            .output()
            .expect(
            "Failed to reload nginx configuration, please ensure nginx is installed and running",
        );
    }
}

impl IngressHost {
    pub fn new(host: &String) -> Self {
        Self {
            host: host.to_owned(),
            paths: vec![],
        }
    }

    pub fn add_path(&mut self, path: &String, svc_ip: &Ipv4Addr, svc_port: &u16) {
        self.paths.push(IngressPath {
            path: path.to_owned(),
            ip: svc_ip.to_owned(),
            port: svc_port.to_owned(),
        });
    }
}

impl ToString for NginxIngressConfig {
    fn to_string(&self) -> String {
        let servers = self.servers.iter().fold(String::new(), |acc, server| {
            acc + server.to_string().as_str()
        });
        let out = CONFIG_BASE.to_string().replace("SERVERS", servers.as_str());
        CONFIG_HEADER.to_string()
            + nginx_config::parse_main(out.as_str())
                .unwrap()
                .to_string()
                .as_str() // format
    }
}

impl ToString for IngressHost {
    fn to_string(&self) -> String {
        let out = SERVER_BASE.to_string();
        let out = out.replace("SERVER_NAME", &self.host);
        let locations = self
            .paths
            .iter()
            .fold(String::new(), |acc, path| acc + path.to_string().as_str());
        out.replace("LOCATIONS", &locations)
    }
}

impl ToString for IngressPath {
    fn to_string(&self) -> String {
        let out = LOCATION_BASE.to_string();
        let out = out.replace("PATH", self.path.as_str());
        let out = out.replace("IP", self.ip.to_string().as_str());
        out.replace("PORT", self.port.to_string().as_str())
    }
}
