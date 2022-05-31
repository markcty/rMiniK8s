use std::{io, net::Ipv4Addr, path::PathBuf, sync::Arc};

use axum::{
    body::Bytes,
    extract::{
        multipart::Field,
        ws::{CloseFrame, Message},
        WebSocketUpgrade,
    },
    http::{Request, Uri},
    response::IntoResponse,
    BoxError,
};
use etcd_client::{GetOptions, GetResponse, WatchOptions, WatchStream, Watcher};
use futures::{SinkExt, Stream, StreamExt, TryStreamExt};
use hyper::{client::HttpConnector, Body, Client};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use resources::{
    config::kubelet::KubeletConfig,
    models::ErrResponse,
    objects::{node::Node, pod::Pod, KubeObject, Object},
};
use tokio::{fs::File, io::BufWriter};
use tokio_tungstenite::{connect_async, tungstenite as ts};
use tokio_util::io::StreamReader;

use crate::{
    etcd::{self, kv_to_str},
    AppState, TMP_DIR,
};

pub async fn etcd_put(app_state: &Arc<AppState>, val: &KubeObject) -> Result<(), ErrResponse> {
    let mut client = app_state.get_client().await?;
    etcd::put(&mut client, val.uri().as_str(), val, None)
        .await
        .map_err(ErrResponse::from)?;
    Ok(())
}

pub async fn etcd_get(app_state: &Arc<AppState>, key: String) -> Result<GetResponse, ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::get(&mut client, &key, None)
        .await
        .map_err(ErrResponse::from)?;
    Ok(res)
}

pub async fn etcd_delete(app_state: &Arc<AppState>, key: String) -> Result<(), ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::delete(&mut client, &key, None)
        .await
        .map_err(ErrResponse::from)?;
    if res.deleted() > 0 {
        Ok(())
    } else {
        Err(ErrResponse::not_found(format!("{} not found", &key), None))
    }
}

pub async fn etcd_get_objects_by_prefix(
    app_state: &Arc<AppState>,
    prefix: String,
    kind: Option<&str>,
) -> Result<Vec<KubeObject>, ErrResponse> {
    let mut client = app_state.get_client().await?;
    let res = etcd::get(&mut client, &prefix, Some(GetOptions::new().with_prefix()))
        .await
        .map_err(ErrResponse::from)?;

    let mut objects: Vec<KubeObject> = Vec::new();
    for kv in res.kvs() {
        let (_, val_str) = kv_to_str(kv)?;
        let object: KubeObject = serde_json::from_str(val_str.as_str()).map_err(|err| {
            ErrResponse::new("failed to deserialize".into(), Some(err.to_string()))
        })?;

        if let Some(kind) = kind {
            if object.kind().eq_ignore_ascii_case(kind) {
                objects.push(object);
            } else {
                tracing::error!(
                    "Object kind error: expected {}, found: {}",
                    kind,
                    object.kind()
                );
                return Err(ErrResponse::new(
                    "There are some errors with the kind of objects".to_string(),
                    Some(format!("expected: {}, found: {}", kind, object.kind())),
                ));
            }
        }
    }
    Ok(objects)
}

pub async fn etcd_get_object(
    app_state: &Arc<AppState>,
    uri: String,
    kind: Option<&str>,
) -> Result<KubeObject, ErrResponse> {
    let etcd_res = etcd_get(app_state, uri).await?;
    let object_str = get_value_str(etcd_res)?;
    let object: KubeObject = serde_json::from_str(object_str.as_str())
        .map_err(|err| ErrResponse::new("failed to deserialize".into(), Some(err.to_string())))?;
    if let Some(kind) = kind {
        if !object.kind().eq_ignore_ascii_case(kind) {
            return Err(ErrResponse::new("get object type error".to_string(), None));
        }
    }
    Ok(object)
}

pub async fn etcd_watch_uri(
    app_state: &Arc<AppState>,
    uri: &str,
) -> Result<(Watcher, WatchStream), ErrResponse> {
    let mut client = app_state.get_client().await?;
    let (watcher, stream) = client
        .watch(uri, Some(WatchOptions::new().with_prefix()))
        .await
        .map_err(|err| {
            ErrResponse::new(
                "Failed to establish watch connection".to_string(),
                Some(err.to_string()),
            )
        })?;
    tracing::info!("Etcd watch created, watch id: {}", watcher.watch_id());
    Ok((watcher, stream))
}

pub fn get_value_str(response: GetResponse) -> Result<String, ErrResponse> {
    if let Some(kv) = response.kvs().first() {
        let (_, val_str) = kv_to_str(kv)?;
        Ok(val_str)
    } else {
        Err(ErrResponse::not_found(
            "value doesn't exist".to_string(),
            None,
        ))
    }
}

pub fn unique_name(name: &str) -> String {
    let mut rng = thread_rng();
    let suffix = (&mut rng)
        .sample_iter(Alphanumeric)
        .take(5)
        .map(char::from)
        .collect::<String>()
        .to_lowercase();
    format!("{}-{}", name, suffix)
}

pub fn gen_service_ip(app_state: &Arc<AppState>) -> Ipv4Addr {
    let mut rng = thread_rng();
    loop {
        let ip = Ipv4Addr::new(172, rng.gen_range(16..32), rng.gen(), rng.gen());
        if !app_state.service_ip_pool.contains(&ip) {
            app_state.service_ip_pool.insert(ip);
            return ip;
        }
    }
}

pub fn gen_rand_host() -> String {
    let mut rng = thread_rng();
    let prefix = (&mut rng)
        .sample_iter(Alphanumeric)
        .take(5)
        .map(char::from)
        .collect::<String>()
        .to_lowercase();
    format!("{}.minik8s.com", prefix)
}

fn unique_filename(filename: &str) -> PathBuf {
    let mut rng = thread_rng();

    let filename = PathBuf::from(filename);
    let mut new_filename = filename.file_stem().unwrap_or_default().to_os_string();
    new_filename.push(
        (&mut rng)
            .sample_iter(Alphanumeric)
            .take(5)
            .map(char::from)
            .collect::<String>()
            .as_str(),
    );

    let mut new_filename = PathBuf::from(new_filename);
    if let Some(ext) = filename.extension() {
        new_filename.set_extension(ext);
    }
    new_filename
}

pub async fn stream_to_tmp_file<S, E>(
    original_filename: &str,
    stream: S,
) -> Result<String, ErrResponse>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    async {
        // Convert the stream into an `AsyncRead`.
        let body_with_io_error = stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
        let body_reader = StreamReader::new(body_with_io_error);
        futures::pin_mut!(body_reader);

        // add unique hash to filename
        let file = unique_filename(original_filename);

        // write file
        let path = std::path::Path::new(TMP_DIR).join(&file);
        let mut file_writer = BufWriter::new(File::create(path).await?);
        tokio::io::copy(&mut body_reader, &mut file_writer).await?;

        // return the new filename
        tracing::info!("add new tmp file: {}", file.display());
        Ok::<String, io::Error>(file.to_str().unwrap_or_default().to_string())
    }
    .await
    .map_err(|err| ErrResponse::new(err.to_string(), None))
}

pub async fn decode_field_json(field: Field<'_>) -> Result<KubeObject, ErrResponse> {
    serde_json::from_str(
        field
            .text()
            .await
            .map_err(|_| ErrResponse::bad_request("Invalid field".to_string(), None))?
            .as_str(),
    )
    .map_err(|_| ErrResponse::bad_request("Failed to deserialize".to_string(), None))
}

pub async fn store_code_file(field: Field<'_>) -> Result<String, ErrResponse> {
    let original_filename = field.file_name().map_or("".to_string(), |n| n.to_owned());
    if !original_filename.ends_with(".zip") {
        let err = ErrResponse::bad_request("Please upload a zip file".to_string(), None);
        return Err(err);
    }

    stream_to_tmp_file(original_filename.as_str(), field).await
}

pub async fn get_pod_node(app_state: &Arc<AppState>, pod: &Pod) -> Option<Node> {
    let node_name = pod.spec.node_name.as_ref();
    match node_name {
        Some(node_name) => {
            let node_object = etcd_get_object(
                app_state,
                format!("/api/v1/nodes/{}", node_name),
                Some("node"),
            )
            .await;
            match node_object {
                Ok(KubeObject::Node(node)) => Some(node),
                _ => None,
            }
        },
        None => None,
    }
}

/// Get rKubelet addr of the pod's node
pub async fn pod_node_rkubelet_addr(
    app_state: Arc<AppState>,
    pod_name: String,
) -> Result<String, ErrResponse> {
    let pod_object = etcd_get_object(
        &app_state,
        format!("/api/v1/pods/{}", pod_name),
        Some("pod"),
    )
    .await;
    match pod_object {
        Ok(KubeObject::Pod(pod)) => {
            let node = get_pod_node(&app_state, &pod).await;
            let node_ip = node
                .as_ref()
                .and_then(|node| node.internal_ip())
                .unwrap_or_else(|| "127.0.0.1".to_string());
            let kubelet_port = node.map_or(KubeletConfig::default().port, |node| {
                node.status.kubelet_port
            });
            Ok(format!("{}:{}", node_ip, kubelet_port))
        },
        _ => Err(ErrResponse::new(String::from("Failed to get pod"), None)),
    }
}

/// Proxy request to rKubelet on the pod's node
pub async fn proxy_to_rkubelet(
    app_state: Arc<AppState>,
    uri: Uri,
    pod_name: String,
    mut request: Request<Body>,
) -> axum::http::Response<Body> {
    let addr = pod_node_rkubelet_addr(app_state.clone(), pod_name).await;
    match addr {
        Ok(addr) => {
            let path = uri.path();
            let path_query = uri.path_and_query().map(|v| v.as_str()).unwrap_or(path);
            let uri = format!("http://{}{}", addr, path_query);
            tracing::debug!("Proxying request to {}", uri);
            *request.uri_mut() = Uri::try_from(uri).unwrap();
            let client = Client::<HttpConnector, Body>::new();
            client.request(request).await.unwrap_or_else(|e| {
                axum::http::Response::new(Body::from(
                    ErrResponse::new(
                        String::from("Failed to connect to rKubelet"),
                        Some(e.to_string()),
                    )
                    .json(),
                ))
            })
        },
        _ => axum::http::Response::new(Body::from(
            ErrResponse::new(String::from("Pod not found"), None).json(),
        )),
    }
}

/// Proxy websocket to rKubelet on the pod's node
pub async fn proxy_ws_to_rkubelet(
    app_state: Arc<AppState>,
    uri: Uri,
    pod_name: String,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ErrResponse> {
    let addr = pod_node_rkubelet_addr(app_state.clone(), pod_name).await;
    match addr {
        Ok(addr) => {
            let path = uri.path();
            let path_query = uri.path_and_query().map(|v| v.as_str()).unwrap_or(path);
            let uri = format!("ws://{}{}", addr, path_query);
            tracing::debug!("Proxying websocket to {}", uri);

            let (stream, _) = connect_async(uri).await.map_err(|e| {
                ErrResponse::new(
                    String::from("Failed to connect to rKubelet"),
                    Some(e.to_string()),
                )
            })?;
            let (mut server_sender, mut server_receiver) = stream.split();

            Ok(ws.on_upgrade(|socket| async move {
                let (mut client_sender, mut client_receiver) = socket.split();

                loop {
                    tokio::select! {
                        Some(msg) = client_receiver.next() => {
                            match msg {
                                Ok(msg) => {
                                    if let Err(err) = server_sender.send(into_tungstenite(msg)).await {
                                        tracing::error!("Failed to send message to rKubelet: {}", err);
                                        break;
                                    }
                                },
                                _ => break,
                            }
                        },
                        Some(msg) = server_receiver.next() => {
                            match msg {
                                Ok(msg) => {
                                    if let Err(err) = client_sender.send(from_tungstenite(msg).unwrap()).await {
                                        tracing::error!("Failed to send message to client: {}", err);
                                        break;
                                    }
                                },
                                _ => break,
                            }
                        },
                        else => break,
                    }
                }
                client_sender.close().await.ok();
            }))
        },
        _ => Err(ErrResponse::new(String::from("Pod not found"), None)),
    }
}

/// Convert axum Message into tunngenite Message,
/// copied from axum::extract::ws since it's private.
fn into_tungstenite(message: Message) -> ts::Message {
    match message {
        Message::Text(text) => ts::Message::Text(text),
        Message::Binary(binary) => ts::Message::Binary(binary),
        Message::Ping(ping) => ts::Message::Ping(ping),
        Message::Pong(pong) => ts::Message::Pong(pong),
        Message::Close(Some(close)) => ts::Message::Close(Some(ts::protocol::CloseFrame {
            code: ts::protocol::frame::coding::CloseCode::from(close.code),
            reason: close.reason,
        })),
        Message::Close(None) => ts::Message::Close(None),
    }
}

/// Convert tunngenite Message into axum Message,
/// copied from axum::extract::ws since it's private.
fn from_tungstenite(message: ts::Message) -> Option<Message> {
    match message {
        ts::Message::Text(text) => Some(Message::Text(text)),
        ts::Message::Binary(binary) => Some(Message::Binary(binary)),
        ts::Message::Ping(ping) => Some(Message::Ping(ping)),
        ts::Message::Pong(pong) => Some(Message::Pong(pong)),
        ts::Message::Close(Some(close)) => Some(Message::Close(Some(CloseFrame {
            code: close.code.into(),
            reason: close.reason,
        }))),
        ts::Message::Close(None) => Some(Message::Close(None)),
        ts::Message::Frame(_) => None,
    }
}
