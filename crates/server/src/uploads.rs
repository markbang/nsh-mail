//! Files staged for a draft (`/v1/uploads`). A client creates an upload with its name, type and
//! size, sends the bytes in chunks (so it can show progress, and no request body is large), then
//! names it in `/v1/send`. Each user's are `uploads/<user id>/<id>` (bytes) and `<id>.json`
//! (metadata); a send removes its own, and the sweeper removes the rest a day after they were
//! added.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use axum::{
    Json,
    body::Bytes,
    extract::{Path as UrlPath, Query, State},
    http::StatusCode,
};
use mail_core::{
    StagedFile,
    types::{DraftAttachment, ErrorCode, Upload},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

use crate::{
    AppState,
    api::{ApiError, ApiResult, AuthUser},
};

/// How long an upload may wait to be sent.
pub const EXPIRES: Duration = Duration::from_secs(24 * 60 * 60);
/// The largest chunk a client sends in one request.
pub const CHUNK_LIMIT: usize = 4 * 1024 * 1024;

pub fn too_large(limit: u64) -> ApiError {
    ApiError::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        ErrorCode::TooLarge,
        format!(
            "attachments can add up to {} MB on this server",
            limit / (1024 * 1024)
        ),
    )
}

fn dir(state: &AppState, user: &AuthUser) -> PathBuf {
    state
        .config
        .data_dir
        .join("uploads")
        .join(user.id.to_string())
}

/// The files of upload `id`; ids are random base64url, never paths.
fn paths(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<(PathBuf, PathBuf)> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(gone());
    }
    let dir = dir(state, user);
    Ok((dir.join(id), dir.join(format!("{id}.json"))))
}

fn gone() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        ErrorCode::NotFound,
        "this attachment is gone (uploads expire after a day); attach the file again",
    )
}

fn io(e: std::io::Error) -> ApiError {
    anyhow::anyhow!("upload storage: {e}").into()
}

async fn meta(state: &AppState, user: &AuthUser, id: &str) -> ApiResult<(Upload, PathBuf, u64)> {
    let (data, json) = paths(state, user, id)?;
    let text = match tokio::fs::read(&json).await {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(gone()),
        Err(e) => return Err(io(e)),
    };
    let upload: Upload =
        serde_json::from_slice(&text).map_err(|e| anyhow::anyhow!("upload metadata: {e}"))?;
    let received = tokio::fs::metadata(&data).await.map_err(io)?.len();
    Ok((upload, data, received))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewUpload {
    filename: String,
    mime_type: String,
    size: u64,
}

pub async fn create(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    Json(n): Json<NewUpload>,
) -> ApiResult<(StatusCode, Json<Upload>)> {
    let limit = s.config.max_attachments;
    if n.size > limit {
        return Err(too_large(limit));
    }
    let filename = n.filename.trim();
    if filename.is_empty() || filename.len() > 1024 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::InvalidInput,
            "a file needs a name of up to 1024 bytes",
        ));
    }
    let upload = Upload {
        id: mail_core::oauth::random_token(16),
        filename: filename.to_string(),
        mime_type: n.mime_type.trim().to_string(),
        size: n.size,
    };
    let (data, json) = paths(&s, &u, &upload.id)?;
    tokio::fs::create_dir_all(dir(&s, &u)).await.map_err(io)?;
    tokio::fs::write(&data, b"").await.map_err(io)?;
    let text = serde_json::to_vec(&upload).map_err(|e| anyhow::anyhow!(e))?;
    tokio::fs::write(&json, text).await.map_err(io)?;
    Ok((StatusCode::CREATED, Json(upload)))
}

#[derive(Deserialize)]
pub struct Offset {
    offset: u64,
}

/// Appends a chunk at `offset`, which must be where the bytes so far end.
pub async fn append(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    UrlPath(id): UrlPath<String>,
    Query(at): Query<Offset>,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    let (upload, data, received) = meta(&s, &u, &id).await?;
    if at.offset != received {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::InvalidInput,
            format!("the upload has {received} bytes; send from there"),
        ));
    }
    if received + body.len() as u64 > upload.size {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::InvalidInput,
            format!("the file is {} bytes, as it was announced", upload.size),
        ));
    }
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(&data)
        .await
        .map_err(io)?;
    file.write_all(&body).await.map_err(io)?;
    file.flush().await.map_err(io)?;
    Ok(Json(json!({ "received": received + body.len() as u64 })))
}

pub async fn delete(
    State(s): State<Arc<AppState>>,
    u: AuthUser,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<StatusCode> {
    let (data, json) = paths(&s, &u, &id)?;
    let _ = tokio::fs::remove_file(&data).await;
    match tokio::fs::remove_file(&json).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(gone()),
        Err(e) => Err(io(e)),
    }
}

/// The files a draft names, read whole, keyed by upload id. Fails on any missing, unfinished,
/// or (together) over the limit.
pub async fn staged(
    s: &AppState,
    u: &AuthUser,
    attachments: &[DraftAttachment],
) -> ApiResult<HashMap<String, StagedFile>> {
    let mut files = HashMap::new();
    let mut total = 0u64;
    for a in attachments {
        let DraftAttachment::Upload { id } = a else {
            continue;
        };
        let (upload, data, received) = meta(s, u, id).await?;
        if received != upload.size {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                ErrorCode::InvalidInput,
                format!("{} is still uploading", upload.filename),
            ));
        }
        total += upload.size;
        if total > s.config.max_attachments {
            return Err(too_large(s.config.max_attachments));
        }
        let data = tokio::fs::read(&data).await.map_err(io)?;
        files.insert(
            upload.id,
            StagedFile {
                filename: upload.filename,
                mime_type: upload.mime_type,
                data,
            },
        );
    }
    Ok(files)
}

/// Removes a sent draft's uploads.
pub async fn remove(s: &AppState, u: &AuthUser, ids: impl Iterator<Item = &str>) {
    for id in ids {
        if let Ok((data, json)) = paths(s, u, id) {
            let _ = tokio::fs::remove_file(data).await;
            let _ = tokio::fs::remove_file(json).await;
        }
    }
}

/// Stages a cancelled message's files again under their ids; a day to send them from now.
pub async fn restore(
    s: &AppState,
    u: &AuthUser,
    files: HashMap<String, StagedFile>,
) -> ApiResult<()> {
    tokio::fs::create_dir_all(dir(s, u)).await.map_err(io)?;
    for (id, file) in files {
        let (data, json) = paths(s, u, &id)?;
        let upload = Upload {
            id,
            filename: file.filename,
            mime_type: file.mime_type,
            size: file.data.len() as u64,
        };
        tokio::fs::write(&data, file.data).await.map_err(io)?;
        let text = serde_json::to_vec(&upload).map_err(|e| anyhow::anyhow!(e))?;
        tokio::fs::write(&json, text).await.map_err(io)?;
    }
    Ok(())
}

/// Removes uploads older than [`EXPIRES`], now and every hour.
pub fn sweep(root: PathBuf) {
    tokio::spawn(async move {
        loop {
            let root = root.clone();
            let swept = tokio::task::spawn_blocking(move || sweep_once(&root)).await;
            if let Ok(Err(e)) = swept {
                tracing::warn!("cannot clean up old uploads: {e}");
            }
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

fn sweep_once(root: &Path) -> std::io::Result<()> {
    let Ok(users) = std::fs::read_dir(root) else {
        return Ok(());
    };
    let now = SystemTime::now();
    for user in users {
        for file in std::fs::read_dir(user?.path())? {
            let file = file?;
            let modified = file.metadata()?.modified()?;
            if now.duration_since(modified).unwrap_or_default() > EXPIRES {
                std::fs::remove_file(file.path())?;
            }
        }
    }
    Ok(())
}
