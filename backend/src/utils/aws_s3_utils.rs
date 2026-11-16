use crate::models::ErrorResponse;
use crate::utils::image_processing::make_icon;
use crate::utils::json_error::json_error;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use rocket::http::Status;
use rocket::serde::json::Json;
use std::error::Error;
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::sync::Semaphore;

// Caps concurrent decodes; each can use ~256 MiB plus resize buffers.
static DECODE_PERMITS: Semaphore = Semaphore::const_new(2);

type BoxError = Box<dyn Error + Send + Sync>;

// Where icons live: S3 when S3_IMAGE_BUCKET is set, else a local dir served at /images.
pub enum Storage {
    S3 { client: Client, bucket: String },
    Local(PathBuf),
}

impl Storage {
    pub async fn from_env() -> Self {
        // docker compose passes unset vars as "", treat that as unset too
        let var = |name| dotenv::var(name).ok().filter(|v| !v.trim().is_empty());
        match var("S3_IMAGE_BUCKET") {
            Some(bucket) => Storage::S3 {
                client: Client::new(&aws_config::load_from_env().await),
                bucket,
            },
            None => Storage::Local(var("UPLOAD_DIR").unwrap_or_else(|| "uploads".into()).into()),
        }
    }

    async fn put_webp(&self, path: &str, body: Vec<u8>) -> Result<(), BoxError> {
        match self {
            Storage::S3 { client, bucket } => {
                client
                    .put_object()
                    .bucket(bucket)
                    .key(path)
                    .content_type("image/webp")
                    .cache_control("public, max-age=31536000, immutable")
                    .body(ByteStream::from(body))
                    .send()
                    .await?;
            }
            Storage::Local(root) => {
                let file = root.join(path);
                tokio::fs::create_dir_all(file.parent().unwrap()).await?;
                tokio::fs::write(file, body).await?;
            }
        }
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), BoxError> {
        match self {
            Storage::S3 { client, bucket } => {
                client
                    .delete_object()
                    .bucket(bucket)
                    .key(path)
                    .send()
                    .await?;
            }
            Storage::Local(root) => match tokio::fs::remove_file(root.join(path)).await {
                Err(e) if e.kind() != ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            },
        }
        Ok(())
    }
}

// Keys are server-generated UUIDs; still refuse anything that could leave the folder.
fn check_path(folder: &str, key: &str) -> Result<(), (Status, Json<ErrorResponse>)> {
    let bad = |s: &str| s.is_empty() || s.contains("..") || s.contains('/') || s.contains('\\');
    if bad(folder) || bad(key) {
        return Err((Status::BadRequest, json_error("Invalid image key")));
    }
    Ok(())
}

// Converts an uploaded image to a 512px icon at "<folder>/<key>" and a 128px one at "<folder>/thumb/<key>".
pub async fn upload_icon(
    storage: &Storage,
    folder: &str,
    key: &str,
    bytes: Vec<u8>,
) -> Result<(), (Status, Json<ErrorResponse>)> {
    check_path(folder, key)?;
    // Never closed, so acquire can't fail. Permit moves into the task so it is held until decoding ends.
    let permit = DECODE_PERMITS.acquire().await.unwrap();
    let icon = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        make_icon(&bytes)
    })
    .await
    .map_err(|_| {
        (
            Status::InternalServerError,
            json_error("Image processing failed"),
        )
    })?
    .map_err(|_| (Status::BadRequest, json_error("Invalid or too large image")))?;

    let (full, thumb) = (
        format!("{}/{}", folder, key),
        format!("{}/thumb/{}", folder, key),
    );
    tokio::try_join!(
        storage.put_webp(&full, icon.full),
        storage.put_webp(&thumb, icon.thumb),
    )
    .map_err(|_| {
        (
            Status::InternalServerError,
            json_error("Icon upload failed"),
        )
    })?;
    Ok(())
}

// Deletes both sizes. The thumb delete is best-effort so a failure there never orphans
// the full-size image the DB still points at; old icons have no thumb anyway.
pub async fn remove_icon(storage: &Storage, folder: &str, key: &str) -> Result<(), BoxError> {
    check_path(folder, key).map_err(|_| "invalid image key")?;
    let _ = storage.delete(&format!("{}/thumb/{}", folder, key)).await;
    storage.delete(&format!("{}/{}", folder, key)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_write_delete_and_path_guard() {
        let root = std::env::temp_dir().join(format!("harmony-storage-{}", uuid::Uuid::new_v4()));
        let storage = Storage::Local(root.clone());

        storage.put_webp("user-icons/abc", vec![1]).await.unwrap();
        storage
            .put_webp("user-icons/thumb/abc", vec![2])
            .await
            .unwrap();
        assert_eq!(std::fs::read(root.join("user-icons/abc")).unwrap(), [1]);
        assert_eq!(
            std::fs::read(root.join("user-icons/thumb/abc")).unwrap(),
            [2]
        );

        remove_icon(&storage, "user-icons", "abc").await.unwrap();
        assert!(!root.join("user-icons/abc").exists());
        assert!(!root.join("user-icons/thumb/abc").exists());
        // already gone: NotFound is ignored
        remove_icon(&storage, "user-icons", "abc").await.unwrap();

        for (folder, key) in [
            ("user-icons", "../x"),
            ("user-icons", "a/b"),
            ("..", "x"),
            ("user-icons", ""),
        ] {
            assert!(check_path(folder, key).is_err());
            assert!(remove_icon(&storage, folder, key).await.is_err());
        }
        assert!(upload_icon(&storage, "user-icons", "..", vec![])
            .await
            .is_err());

        std::fs::remove_dir_all(root).unwrap();
    }
}
