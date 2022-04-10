use pod_manager::PodManager;
use resources::objects::{pod::*, KubeObject, KubeSpec, Metadata};

mod pod_manager;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let pod_spec = KubeSpec::Pod(PodSpec {
        containers: vec![
            Container {
                name: "nginx".to_string(),
                image: "nginx:latest".to_string(),
                ports: vec![ContainerPort {
                    container_port: 6379,
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
    let pod = KubeObject {
        metadata: Metadata {
            name: "nginx".to_string(),
        },
        spec: pod_spec,
        status: None,
    };
    let pod_manager = PodManager::new();
    pod_manager.create(&pod).await.unwrap();
}
