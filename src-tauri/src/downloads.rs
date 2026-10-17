//! Local models downloaded on request (Models page) instead of implicitly on
//! first use: big MLX checkpoints for the Apple Silicon GPU. Files are pinned
//! to a Hugging Face revision and land in the app's data dir; the download
//! reports progress (`model-download`), resumes from a `.part` file after a
//! network error and can be cancelled.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::i18n::{t, tf};

pub struct Downloadable {
    /// Local model name (`local:<name>` in settings).
    pub name: &'static str,
    repo: &'static str,
    revision: &'static str,
    /// File name and exact size in bytes.
    files: &'static [(&'static str, u64)],
}

const TURBO: Downloadable = Downloadable {
    name: "turbo",
    repo: "mlx-community/whisper-large-v3-turbo",
    revision: "a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb",
    files: &[("config.json", 268), ("weights.safetensors", 1_613_977_612)],
};

/// Downloadable models this machine can run (MLX needs Apple Silicon).
fn catalog() -> &'static [Downloadable] {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        &[TURBO]
    } else {
        &[]
    }
}

pub fn find(name: &str) -> Option<&'static Downloadable> {
    catalog().iter().find(|d| d.name == name)
}

/// Folder holding the model's files.
pub fn dir(app: &AppHandle, d: &Downloadable) -> Result<PathBuf> {
    let base = app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow!(tf("error.dataDir", &[("error", &e)])))?;
    let folder = d.repo.rsplit('/').next().unwrap_or(d.name);
    Ok(base.join("models").join(folder))
}

pub fn is_downloaded(app: &AppHandle, d: &Downloadable) -> bool {
    dir(app, d).is_ok_and(|dir| d.files.iter().all(|(f, size)| file_len(&dir.join(f)) == Some(*size)))
}

fn file_len(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| m.len())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModel {
    id: String,
    size: u64,
    downloaded: bool,
    downloading: bool,
}

/// The downloadable models available here and where each one stands.
pub fn list(app: &AppHandle) -> Vec<LocalModel> {
    let active = *ACTIVE.lock().unwrap();
    catalog()
        .iter()
        .map(|d| LocalModel {
            id: format!("{}{}", crate::local::PREFIX, d.name),
            size: d.files.iter().map(|(_, s)| s).sum(),
            downloaded: is_downloaded(app, d),
            downloading: active == Some(d.name),
        })
        .collect()
}

/// Model being downloaded (one at a time) and its cancel flag.
static ACTIVE: Mutex<Option<&'static str>> = Mutex::new(None);
static CANCEL: AtomicBool = AtomicBool::new(false);

/// Error text of a download cancelled on purpose. A machine-readable sentinel,
/// not shown to the user: the settings window matches it (`DOWNLOAD_CANCELLED`
/// in `src/lib/ipc.ts`) to stay silent, so it is deliberately not translated.
/// Keep both sides in sync.
pub const CANCELLED: &str = "download-cancelled";

/// `model-download` event payload. `phase` is `"engine"` while the Python
/// packages install (no byte count) and `"model"` while the weights download.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: String,
    phase: &'static str,
    received: u64,
    total: u64,
}

/// Install the engine and download `name`'s files, emitting progress. Every
/// window hears `local-models-changed` when it ends, however it ends.
pub async fn download(app: &AppHandle, name: &str) -> Result<()> {
    let d = find(name).ok_or_else(|| anyhow!(tf("error.download.notDownloadable", &[("model", &name)])))?;
    {
        let mut active = ACTIVE.lock().unwrap();
        if active.is_some() {
            bail!(t("error.download.busy"));
        }
        *active = Some(d.name);
    }
    CANCEL.store(false, Ordering::SeqCst);
    let _ = app.emit("local-models-changed", ());

    let result = fetch(app, d).await;
    if result.is_err() && CANCEL.load(Ordering::SeqCst) {
        // Cancelled on purpose: don't keep a half-downloaded checkpoint.
        if let Ok(dir) = dir(app, d) {
            for (file, _) in d.files {
                let _ = std::fs::remove_file(dir.join(format!("{file}.part")));
            }
        }
    }
    *ACTIVE.lock().unwrap() = None;
    let _ = app.emit("local-models-changed", ());
    result
}

/// Stop the download in progress (it fails with [`CANCELLED`]).
pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn cancelled() -> Result<()> {
    if CANCEL.load(Ordering::SeqCst) {
        bail!(CANCELLED);
    }
    Ok(())
}

async fn fetch(app: &AppHandle, d: &'static Downloadable) -> Result<()> {
    let id = format!("{}{}", crate::local::PREFIX, d.name);
    let total: u64 = d.files.iter().map(|(_, s)| s).sum();
    let emit = |phase, received| {
        let _ = app.emit("model-download", Progress { id: id.clone(), phase, received, total });
    };

    emit("engine", 0);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || crate::local::ensure_mlx(&handle))
        .await
        .map_err(|e| anyhow!(tf("error.download.engine", &[("error", &e)])))??;
    cancelled()?;

    emit("model", 0);
    let mut last_emit = Instant::now();
    fetch_files(&dir(app, d)?, d, |received| {
        if received == total || last_emit.elapsed() >= Duration::from_millis(100) {
            last_emit = Instant::now();
            emit("model", received);
        }
    })
    .await
}

/// Download `d`'s missing files into `dir`, resuming `.part` files, calling
/// `progress` with the bytes received so far (all files).
async fn fetch_files(dir: &Path, d: &Downloadable, mut progress: impl FnMut(u64)) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| anyhow!(tf("error.download.createDir", &[("error", &e)])))?;
    let save_failed = |e: std::io::Error| anyhow!(tf("error.download.save", &[("error", &e)]));
    let client = reqwest::Client::new();
    let mut done = 0;

    for &(file, size) in d.files {
        let dest = dir.join(file);
        if file_len(&dest) == Some(size) {
            done += size;
            continue;
        }
        let part = dir.join(format!("{file}.part"));
        let mut have = file_len(&part).filter(|&n| n < size).unwrap_or(0);

        let url = format!("https://huggingface.co/{}/resolve/{}/{file}", d.repo, d.revision);
        let mut req = client.get(&url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut resp = req
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| anyhow!(tf("error.download.file", &[("file", &file), ("error", &e)])))?;
        let resuming = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if !resuming {
            have = 0;
        }
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resuming)
            .truncate(!resuming)
            .open(&part)
            .map_err(|e| anyhow!(tf("error.download.open", &[("path", &part.display()), ("error", &e)])))?;

        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| anyhow!(tf("error.download.file", &[("file", &file), ("error", &e)])))?
        {
            cancelled()?;
            out.write_all(&chunk).map_err(save_failed)?;
            have += chunk.len() as u64;
            progress(done + have);
        }
        out.flush().map_err(save_failed)?;
        drop(out);
        if have != size {
            bail!(tf(
                "error.download.incomplete",
                &[("file", &file), ("received", &have), ("size", &size)]
            ));
        }
        std::fs::rename(&part, &dest).map_err(save_failed)?;
        done += size;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG_ONLY: Downloadable = Downloadable {
        name: "turbo",
        repo: TURBO.repo,
        revision: TURBO.revision,
        files: &[("config.json", 268)],
    };

    /// Hits huggingface.co: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_and_resumes_a_partial_file() {
        let dir = std::env::temp_dir().join(format!("wa-dl-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let run = |dir: &Path| {
            let mut last = 0;
            tauri::async_runtime::block_on(fetch_files(dir, &CONFIG_ONLY, |n| last = n)).unwrap();
            last
        };

        assert_eq!(run(&dir), 268);
        let full = std::fs::read(dir.join("config.json")).unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&full).is_ok());

        // A cut-off download picks up where it stopped.
        std::fs::remove_file(dir.join("config.json")).unwrap();
        std::fs::write(dir.join("config.json.part"), &full[..100]).unwrap();
        assert_eq!(run(&dir), 268);
        assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), full);
        assert!(!dir.join("config.json.part").exists());

        // Already there: nothing to fetch.
        assert_eq!(run(&dir), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
