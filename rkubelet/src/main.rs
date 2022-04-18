use resources::objects::{
    pod::{Pod as PodResource, *},
    KubeObject, KubeResource, Metadata,
};

mod config;
mod docker;
mod pod;

use pod::Pod;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let pod_spec = PodSpec {
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
    };
    let object = KubeObject {
        metadata: Metadata {
            name: "nginx".to_string(),
            ..Default::default()
        },
        resource: KubeResource::Pod(PodResource {
            spec: pod_spec,
            status: None,
        }),
    };
    let mut pod = Pod::create(object).await.unwrap();
    println!("{:#?}", pod);
    pod.start().await.unwrap();
    pod.update_status().await.unwrap();
    println!("{:#?}", pod);
}
