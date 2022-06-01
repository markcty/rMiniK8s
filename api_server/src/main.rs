use std::{net::Ipv4Addr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    http::StatusCode,
    routing::{get, get_service, post},
    Extension, Router,
};
use config::Config;
use dashmap::DashSet;
use etcd::create_etcd_pool;
use serde::Deserialize;
use tokio::fs;
use tower_http::services::ServeDir;

mod etcd;
mod handler;

const TMP_DIR: &str = "/tmp/minik8s";
const METRICS_SERVER_CONFIG: &str = "/etc/prometheus/prometheus.yml";

#[derive(Debug, Clone, Deserialize)]
struct ServerConfig {
    #[serde(default = "default_log_level")]
    log_level: String,
    etcd_endpoint: String,
    metrics_server: String,
}
fn default_log_level() -> String {
    "Info".to_string()
}

pub struct AppState {
    etcd_pool: etcd::EtcdPool,
    service_ip_pool: DashSet<Ipv4Addr>,
    config: ServerConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    // read config
    let config = Config::builder()
        .add_source(config::File::with_name("/etc/rminik8s/api_server.yaml").required(false))
        .add_source(config::Environment::default())
        .build()?
        .try_deserialize::<ServerConfig>()
        .with_context(|| "Failed to parse config".to_string())?;

    // init tracing
    std::env::set_var("RUST_LOG", format!("api_server={}", config.log_level));
    tracing_subscriber::fmt::init();

    // init app state
    let app_state = AppState::from_config(&config)?;
    let shared_state = Arc::new(app_state);

    #[rustfmt::skip]
    let pod_routes = Router::new().nest(
        "/pods",
        Router::new()
            .route("/",
                   get(handler::pod::list)
                       .post(handler::pod::create))
            .nest("/:name",
                Router::new()
                    .route("/",
                    get(handler::pod::get)
                        .put(handler::pod::replace)
                        .delete(handler::pod::delete))
                    .route("/logs",
                        get(handler::pod::pod_logs))
                    .nest("/containers/:container_name",
                        Router::new()
                        .route("/logs",
                           get(handler::pod::container_logs))
                        .route("/exec",
                           get(handler::pod::container_exec))
                    )
            ),
    );

    #[rustfmt::skip]
    let rs_routes = Router::new().nest(
        "/replicasets",
        Router::new()
            .route("/",
                   get(handler::replica_set::list)
                       .post(handler::replica_set::create))
            .route("/:name",
                   get(handler::replica_set::get)
                       .put(handler::replica_set::update)
                       .patch(handler::replica_set::patch)
                       .delete(handler::replica_set::delete),
            ),
    );

    #[rustfmt::skip]
    let service_routes = Router::new().nest(
        "/services",
        Router::new()
            .route("/",
                   get(handler::service::list)
                       .post(handler::service::create))
            .route("/:name",
                   get(handler::service::get)
                       .put(handler::service::update)
                       .delete(handler::service::delete),
            ),
    );

    #[rustfmt::skip]
    let hpa_routes = Router::new().nest(
        "/horizontalpodautoscalers",
        Router::new()
            .route("/",
                   get(handler::hpa::list)
                       .post(handler::hpa::create))
            .route("/:name",
                   get(handler::hpa::get)
                       .put(handler::hpa::update)
                       .patch(handler::hpa::patch)
                       .delete(handler::hpa::delete),
            ),
    );

    #[rustfmt::skip]
    let ingress_route = Router::new().nest(
        "/ingresses",
        Router::new()
            .route("/", get(handler::ingress::list))
            .route(
                "/:name",
                post(handler::ingress::create)
                    .get(handler::ingress::get)
                    .put(handler::ingress::update)
                    .delete(handler::ingress::delete),
            ),
    );

    #[rustfmt::skip]
    let gpujob_routes = Router::new().nest(
        "/gpujobs",
        Router::new()
            .route(
                "/",
                get(handler::gpu_job::list)
                .post(handler::gpu_job::create),
            )
            .nest(
                "/:name",
                Router::new()
                    .route("/",
                    get(handler::gpu_job::get)
                        .put(handler::gpu_job::update)
                        .patch(handler::gpu_job::patch)
                        .delete(handler::gpu_job::delete))
                    .route("/logs",
                        get(handler::gpu_job::job_logs))
            ),
    );

    #[rustfmt::skip]
    let node_routes = Router::new().nest(
        "/nodes",
        Router::new()
            .route("/", get(handler::node::list))
            .route("/:name",
                   get(handler::node::get)
                       .put(handler::node::update)
                       .delete(handler::node::delete),
            ),
    );

    #[rustfmt::skip]
    let watch_routes = Router::new().nest(
        "/watch",
        Router::new()
            .route("/nodes", get(handler::node::watch_all))
            .route("/pods", get(handler::pod::watch_all))
            .route("/replicasets", get(handler::replica_set::watch_all))
            .route("/services", get(handler::service::watch_all))
            .route("/ingresses",get(handler::ingress::watch_all))
            .route("/horizontalpodautoscalers", get(handler::hpa::watch_all))
            .route("/gpujobs", get(handler::gpu_job::watch_all))
            .route("/functions", get(handler::function::watch_all))
            .route("/functions/:name", get(handler::function::watch_one)),
    );

    #[rustfmt::skip]
    let function_routes = Router::new().nest(
        "/functions",
        Router::new()
            .route(
                "/",
                get(handler::function::list).post(handler::function::create),
            )
            .route(
                "/:name",
                get(handler::function::get)
                    .put(handler::function::update)
                    .delete(handler::function::delete),
            ),
    );

    #[rustfmt::skip]
    let metrics_routes = Router::new().nest(
        "/metrics",
        Router::new()
            .route("/pods", get(handler::metrics::list_pods))
            .route("/functions/:name", get(handler::metrics::get_function))
            .route(
                "/targets",
                post(handler::metrics::add_target)
                    .delete(handler::metrics::remove_target)
            )
    );

    #[rustfmt::skip]
    let workflow_routes = Router::new().nest(
        "/workflows",
        Router::new()
            .route(
                "/",
                get(handler::workflow::list)
                    .post(handler::workflow::create),
            )
            .route(
                "/:name",
                get(handler::workflow::get)
                    .put(handler::workflow::update)
                    .delete(handler::workflow::delete),
            ),
    );

    // tmp file server
    fs::create_dir_all(TMP_DIR).await?;
    let tmp_file_service = get_service(ServeDir::new(TMP_DIR)).handle_error(|error| async move {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unhandled internal error: {}", error),
        )
    });

    let app = Router::new()
        .nest(
            "/api/v1",
            Router::new()
                .merge(pod_routes)
                .merge(rs_routes)
                .merge(service_routes)
                .merge(ingress_route)
                .merge(hpa_routes)
                .merge(watch_routes)
                .merge(metrics_routes)
                .merge(node_routes)
                .merge(gpujob_routes)
                .merge(function_routes)
                .merge(workflow_routes)
                .nest("/tmp", tmp_file_service)
                .route("/bindings", post(handler::binding::bind)),
        )
        .layer(Extension(shared_state));

    tracing::info!("Listening at 0.0.0.0:8080");
    axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown())
        .await
        .unwrap();

    Ok(())
}

impl AppState {
    fn from_config(config: &ServerConfig) -> Result<AppState> {
        let pool = create_etcd_pool(config.etcd_endpoint.as_str())
            .with_context(|| "Failed to create etcd client pool".to_string())?;

        Ok(AppState {
            etcd_pool: pool,
            service_ip_pool: DashSet::new(),
            config: config.to_owned(),
        })
    }
}

async fn shutdown() {
    tokio::signal::ctrl_c()
        .await
        .expect("expect tokio signal ctrl-c");
    tracing::info!("Shutting Down");
}
