//! Local transcription with faster-whisper (CTranslate2, CPU int8 or NVIDIA
//! CUDA float16).
//!
//! faster-whisper is a Python library, so the app keeps a private virtualenv in
//! its local data dir and runs a long-lived worker (`resources/fw_server.py`)
//! that holds the loaded models in memory. The venv is created and the package
//! installed on first use; models come from the shared Hugging Face cache.
//! Downloaded MLX models (`crate::downloads`) run through the same worker with
//! mlx-whisper, on the Apple Silicon GPU. The CUDA libraries are pip packages
//! added to the same venv on demand; any GPU failure falls back to the CPU.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::i18n::{t, tf};

/// Model ids in the settings / dropdown use this prefix (e.g. `local:medium`).
pub const PREFIX: &str = "local:";

const SERVER_SRC: &str = include_str!("../resources/fw_server.py");

/// The running worker, shared by every transcription (one request at a time).
static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

/// Held while pip changes the venv (worker start-up and model downloads).
static INSTALL: Mutex<()> = Mutex::new(());

/// Set when CUDA failed this session: later requests go straight to the CPU.
static CUDA_FAILED: AtomicBool = AtomicBool::new(false);

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default)]
    text: String,
    #[serde(default)]
    error: String,
}

/// NVIDIA GPU state for the Models page.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    /// Whether CUDA can be offered at all (never on macOS).
    pub supported: bool,
    /// First NVIDIA GPU reported by `nvidia-smi`.
    pub name: Option<String>,
    /// The CUDA libraries are already installed in the venv.
    pub cuda_ready: bool,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// `local:medium` → `Some("medium")`.
pub fn model_name(id: &str) -> Option<&str> {
    id.strip_prefix(PREFIX)
}

/// Whether the private venv still has to be created / populated.
pub fn needs_setup(app: &AppHandle) -> bool {
    venv_dir(app).map(|d| !d.join(".ready").exists()).unwrap_or(true)
}

/// Whether the CUDA libraries still have to be downloaded for the device the
/// settings resolve to (the user picked the GPU explicitly).
pub fn needs_cuda_setup(app: &AppHandle) -> bool {
    resolve_device(app) == "cuda" && !cuda_ready(app)
}

/// Forget a CUDA failure so the GPU is tried again (the setting changed).
pub fn reset_cuda_failure() {
    CUDA_FAILED.store(false, Ordering::Relaxed);
}

/// NVIDIA GPU and CUDA-library state, for the settings UI (blocking).
pub fn gpu_info(app: &AppHandle) -> GpuInfo {
    if cfg!(target_os = "macos") {
        return GpuInfo { supported: false, name: None, cuda_ready: false };
    }
    GpuInfo { supported: true, name: nvidia_gpu().map(str::to_string), cuda_ready: cuda_ready(app) }
}

/// Name of the first NVIDIA GPU, asked to `nvidia-smi` once per session.
fn nvidia_gpu() -> Option<&'static str> {
    static GPU: OnceLock<Option<String>> = OnceLock::new();
    GPU.get_or_init(|| {
        let out = command(Path::new("nvidia-smi"))
            .args(["--query-gpu=name", "--format=csv,noheader"])
            .stdin(Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let name = text.lines().next()?.trim();
        (out.status.success() && !name.is_empty()).then(|| name.to_string())
    })
    .as_deref()
}

fn cuda_ready(app: &AppHandle) -> bool {
    venv_dir(app).map(|d| d.join(".ready-cuda").exists()).unwrap_or(false)
}

/// `"cuda"` or `"cpu"`: where the next request should run. `auto` only picks
/// the GPU when its libraries are already installed, so it never starts the
/// big download by itself.
fn resolve_device(app: &AppHandle) -> &'static str {
    if cfg!(target_os = "macos") || CUDA_FAILED.load(Ordering::Relaxed) {
        return "cpu";
    }
    let setting = app.state::<crate::state::AppState>().settings.lock().unwrap().local_device.clone();
    match setting.as_str() {
        "cuda" => "cuda",
        "auto" if nvidia_gpu().is_some() && cuda_ready(app) => "cuda",
        _ => "cpu",
    }
}

/// Load a model ahead of time so the first dictation doesn't pay for it.
/// A model still to be downloaded is skipped.
pub fn warm(app: &AppHandle, model: &str) -> Result<()> {
    if crate::downloads::find(model).is_some_and(|d| !crate::downloads::is_downloaded(app, d)) {
        return Ok(());
    }
    let mut req = json!({ "cmd": "load", "model": model });
    add_engine(app, model, &mut req)?;
    request_with_fallback(app, req).map(|_| ())
}

/// Point the worker at a downloaded MLX model's folder (`"mlx"`), making sure
/// it is complete and mlx-whisper is installed.
fn add_engine(app: &AppHandle, model: &str, req: &mut serde_json::Value) -> Result<()> {
    let Some(d) = crate::downloads::find(model) else {
        return Ok(());
    };
    if !crate::downloads::is_downloaded(app, d) {
        return Err(anyhow!(t("error.local.notDownloaded")));
    }
    ensure_mlx(app)?;
    req["mlx"] = json!(crate::downloads::dir(app, d)?.to_string_lossy());
    Ok(())
}

/// Transcribe a WAV buffer with the given faster-whisper model (blocking).
pub fn transcribe(
    app: &AppHandle,
    model: &str,
    wav: &[u8],
    opts: &crate::whisper::Options,
) -> Result<String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("whisperaround-{stamp}.wav"));
    let mut req = json!({
        "cmd": "transcribe",
        "model": model,
        "path": path.to_string_lossy(),
        "language": opts.language,
        "prompt": opts.prompt,
    });
    add_engine(app, model, &mut req)?;
    std::fs::write(&path, wav).map_err(|e| anyhow!(tf("error.local.writeAudio", &[("error", &e)])))?;
    let result = request_with_fallback(app, req);
    let _ = std::fs::remove_file(&path);
    result
}

/// Run `req` on the resolved device. If the GPU fails (libraries missing, no
/// usable driver, worker crash...) retry once on the CPU, stay there for the
/// rest of the session and tell the user.
fn request_with_fallback(app: &AppHandle, req: serde_json::Value) -> Result<String> {
    if resolve_device(app) == "cuda" {
        let mut gpu = req.clone();
        gpu["device"] = json!("cuda");
        match ensure_cuda(app).and_then(|()| request(app, gpu)) {
            Ok(text) => return Ok(text),
            Err(e) => {
                CUDA_FAILED.store(true, Ordering::Relaxed);
                let _ = app.emit("notice", tf("notice.cudaFallback", &[("error", &e)]));
            }
        }
    }
    let mut cpu = req;
    cpu["device"] = json!("cpu");
    request(app, cpu)
}

/// Send one request to the worker, (re)starting it if needed.
fn request(app: &AppHandle, req: serde_json::Value) -> Result<String> {
    let mut guard = WORKER.lock().unwrap();
    if guard.is_none() {
        *guard = Some(spawn_worker(app)?);
    }
    let worker = guard.as_mut().unwrap();

    let io = (|| -> Result<Reply> {
        writeln!(worker.stdin, "{req}")?;
        worker.stdin.flush()?;
        let mut line = String::new();
        if worker.stdout.read_line(&mut line)? == 0 {
            return Err(anyhow!(t("error.local.workerExited")));
        }
        Ok(serde_json::from_str(&line)?)
    })();

    match io {
        Ok(r) if r.ok => Ok(r.text),
        Ok(r) => Err(anyhow!(tf("error.local.failed", &[("error", &r.error)]))),
        Err(e) => {
            // Broken pipe / crash: drop it so the next request starts fresh.
            *guard = None;
            Err(e)
        }
    }
}

fn spawn_worker(app: &AppHandle) -> Result<Worker> {
    let python = ensure_venv(app)?;
    let script = venv_dir(app)?.join("fw_server.py");
    std::fs::write(&script, SERVER_SRC)
        .map_err(|e| anyhow!(tf("error.local.writeScript", &[("error", &e)])))?;

    let mut cmd = command(&python);
    #[cfg(target_os = "linux")]
    set_library_path(&mut cmd, &venv_dir(app)?);
    let mut child = cmd
        .arg("-u")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| anyhow!(tf("error.local.start", &[("error", &e)])))?;
    let stdin = child.stdin.take().ok_or_else(|| anyhow!(t("error.local.noPipe")))?;
    let stdout = BufReader::new(child.stdout.take().ok_or_else(|| anyhow!(t("error.local.noPipe")))?);
    Ok(Worker { child, stdin, stdout })
}

/// Let the dynamic loader find the pip-installed NVIDIA libraries. It reads
/// `LD_LIBRARY_PATH` at process start, so the worker can't set it itself.
#[cfg(target_os = "linux")]
fn set_library_path(cmd: &mut Command, venv: &Path) {
    let mut dirs = Vec::new();
    let pythons = std::fs::read_dir(venv.join("lib")).into_iter().flatten().flatten();
    for python in pythons {
        let nvidia = python.path().join("site-packages").join("nvidia");
        for pkg in std::fs::read_dir(nvidia).into_iter().flatten().flatten() {
            dirs.push(pkg.path().join("lib"));
        }
    }
    dirs.extend(std::env::var_os("LD_LIBRARY_PATH").iter().flat_map(std::env::split_paths));
    if let Ok(joined) = std::env::join_paths(dirs) {
        cmd.env("LD_LIBRARY_PATH", joined);
    }
}

fn venv_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow!(tf("error.dataDir", &[("error", &e)])))?;
    Ok(dir.join("fw-venv"))
}

fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

/// Make sure the private venv exists with faster-whisper installed and return
/// its interpreter. Slow only the first time (creates venv + pip install).
fn ensure_venv(app: &AppHandle) -> Result<PathBuf> {
    let _install = INSTALL.lock().unwrap();
    venv_with_faster_whisper(app)
}

fn venv_with_faster_whisper(app: &AppHandle) -> Result<PathBuf> {
    let venv = venv_dir(app)?;
    let python = venv_python(&venv);
    let ready = venv.join(".ready");
    if ready.exists() && python.exists() {
        return Ok(python);
    }

    if !python.exists() {
        std::fs::create_dir_all(&venv)?;
        create_venv(&venv)?;
    }
    run(
        command(&python).args(["-m", "pip", "install", "--disable-pip-version-check", "faster-whisper"]),
        "pip install faster-whisper",
    )?;
    std::fs::write(&ready, "")?;
    Ok(python)
}

/// Add mlx-whisper to the venv for the downloaded MLX models. It is installed
/// without its declared dependencies, which pull in torch just for
/// its checkpoint converter; the ones transcription needs are listed here.
pub fn ensure_mlx(app: &AppHandle) -> Result<()> {
    let _install = INSTALL.lock().unwrap();
    let ready = venv_dir(app)?.join(".ready-mlx");
    if ready.exists() {
        return Ok(());
    }
    let python = venv_with_faster_whisper(app)?;
    run(
        command(&python).args(["-m", "pip", "install", "--disable-pip-version-check"]).args([
            "mlx",
            "numba",
            "numpy",
            "scipy",
            "tqdm",
            "more-itertools",
            "tiktoken",
            "huggingface_hub",
        ]),
        "pip install mlx",
    )?;
    run(
        command(&python).args(["-m", "pip", "install", "--disable-pip-version-check", "--no-deps", "mlx-whisper"]),
        "pip install mlx-whisper",
    )?;
    std::fs::write(&ready, "")?;
    Ok(())
}

/// Add the NVIDIA CUDA libraries (cuBLAS and cuDNN 9, which CTranslate2 4.x
/// expects) to the venv for running models on the GPU. A running worker is
/// dropped afterwards so the next one starts with the library paths set.
pub fn ensure_cuda(app: &AppHandle) -> Result<()> {
    if install_cuda(app)? {
        // Not under INSTALL: `request` takes WORKER first, then INSTALL.
        *WORKER.lock().unwrap() = None;
    }
    Ok(())
}

/// Returns whether something was installed.
fn install_cuda(app: &AppHandle) -> Result<bool> {
    let _install = INSTALL.lock().unwrap();
    let ready = venv_dir(app)?.join(".ready-cuda");
    if ready.exists() {
        return Ok(false);
    }
    let python = venv_with_faster_whisper(app)?;
    run(
        command(&python)
            .args(["-m", "pip", "install", "--disable-pip-version-check"])
            .args(["nvidia-cublas-cu12", "nvidia-cudnn-cu12>=9,<10"]),
        "pip install nvidia-cublas-cu12 nvidia-cudnn-cu12",
    )?;
    std::fs::write(&ready, "")?;
    Ok(true)
}

/// Translation key for where to get Python.
#[cfg(windows)]
const PYTHON_HINT: &str = "error.python.hintWindows";
#[cfg(not(windows))]
const PYTHON_HINT: &str = "error.python.hintMac";

/// Where Python usually lives on macOS (Homebrew on Apple Silicon / Intel,
/// the python.org installer). Apps opened from Finder get a bare
/// `/usr/bin:/bin:/usr/sbin:/sbin` PATH that misses all of them, leaving only
/// the old Command Line Tools Python (or a prompt to install them).
#[cfg(target_os = "macos")]
const MACOS_PYTHON_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/Library/Frameworks/Python.framework/Versions/Current/bin",
];

/// System Python launchers to try, in order of preference.
fn python_candidates() -> &'static [(&'static str, &'static [&'static str])] {
    if cfg!(windows) {
        &[("py", &["-3"]), ("python", &[]), ("python3", &[])]
    } else {
        &[("python3", &[]), ("python", &[])]
    }
}

/// Version string (e.g. `"Python 3.12.7"`) of the first working system
/// Python 3, or `None`. On Windows the Microsoft Store `python.exe` alias that
/// exists before Python is installed exits with an error, so it counts as
/// missing.
pub fn detect_python() -> Option<String> {
    python_candidates().iter().find_map(|(exe, pre)| {
        let out = command(Path::new(exe))
            .args(*pre)
            .arg("--version")
            .stdin(Stdio::null())
            .output()
            .ok()?;
        // Python 2 printed its version on stderr; 3.x uses stdout.
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && text.starts_with("Python 3")).then_some(text)
    })
}

/// Create the venv with the first system Python that works.
fn create_venv(venv: &Path) -> Result<()> {
    let mut last_err = anyhow!(t("error.python.notFound"));
    for (exe, pre) in python_candidates() {
        let mut cmd = command(Path::new(exe));
        cmd.args(*pre).arg("-m").arg("venv").arg(venv);
        match run(&mut cmd, "python -m venv") {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
    }
    Err(anyhow!(tf("error.python.missing", &[("hint", &t(PYTHON_HINT)), ("error", &last_err)])))
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| anyhow!(tf("error.command.spawn", &[("command", &what), ("error", &e)])))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
    Err(anyhow!(tf("error.command.failed", &[("command", &what), ("output", &tail)])))
}

/// A `Command` with UTF-8 stdio and, on Windows, no flashing console window.
fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("PYTHONUTF8", "1").env("PYTHONIOENCODING", "utf-8");
    // A changed child PATH is also the one used to look up `program`.
    #[cfg(target_os = "macos")]
    cmd.env("PATH", macos_path(std::env::var_os("PATH")));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// `PATH` with the usual Python install dirs first, then the inherited one.
#[cfg(target_os = "macos")]
fn macos_path(inherited: Option<std::ffi::OsString>) -> std::ffi::OsString {
    let extra = MACOS_PYTHON_DIRS.iter().map(PathBuf::from);
    let rest = inherited
        .as_deref()
        .map(|p| std::env::split_paths(p).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let dirs: Vec<PathBuf> = extra.chain(rest).filter(|d| seen.insert(d.clone())).collect();
    std::env::join_paths(dirs).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_prefixed_ids_are_local() {
        assert_eq!(model_name("local:medium"), Some("medium"));
        assert_eq!(model_name("whisper-1"), None);
        assert_eq!(model_name("gpt-4o-mini-transcribe"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_path_puts_python_dirs_first_without_duplicates() {
        let path = macos_path(Some("/usr/bin:/opt/homebrew/bin:/bin".into()));
        assert_eq!(
            path,
            "/opt/homebrew/bin:/usr/local/bin:/Library/Frameworks/Python.framework/Versions/Current/bin:/usr/bin:/bin"
        );
    }
}
