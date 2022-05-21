use std::{io, net::Ipv4Addr, path::PathBuf, sync::Arc};

use axum::{body::Bytes, BoxError};
use etcd_client::{GetOptions, GetResponse, WatchOptions, WatchStream, Watcher};
use futures::{Stream, TryStreamExt};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use resources::{
    models::ErrResponse,
    objects::{KubeObject, Object},
};
use tokio::{fs::File, io::BufWriter};
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
