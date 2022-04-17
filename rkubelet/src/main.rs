use bollard::Docker;
use lazy_static::lazy_static;
use resources::objects::{pod::*, KubeObject, KubeSpec, Metadata};

mod docker;
mod pod;

use pod::Pod;

lazy_static! {
    pub static ref DOCKER: Docker =
        Docker::connect_with_local_defaults().expect("Failed to connect to docker daemon");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let pod_spec = KubeSpec::Pod(PodSpec {
        containers: vec![
            Container {
                name: "nginx".to_string(),
                image: "nginx:latest".to_string(),
                ports: vec![ContainerPort {
                    container_port: 80,
                }],
            },
            Container {
                name: "redis".to_string(),
                image: "redis:latest".to_string(),
                ports: vec![ContainerPort {
                    container_port: 6379,
                }],
            },
        ],
    });
    let object = KubeObject {
        metadata: Metadata {
            name: "nginx".to_string(),
        },
        spec: pod_spec,
        status: None,
    };
    let pod = Pod::create(object).await.unwrap();
    println!("{:#?}", pod);
    pod.start().await.unwrap();
}
