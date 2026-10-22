//! Meeting mode: a long recording of a video call, transcribed in chunks while
//! it runs and saved to the history as a meeting. Unlike dictation it never
//! pastes anything.
//!
//! Two threads do the work. The *chunker* drains the [`Recorder`] about once a
//! second, keeps one pending buffer per track and cuts a chunk at a pause in
//! the speech (see [`cut_point`]). The *transcriber* receives the chunks over
//! a channel and transcribes them one at a time, appending [`Segment`]s that
//! the UI follows through the `meeting` event. When the recording stops, the
//! chunker flushes what is left and closes the channel; the transcriber then
//! drains the queue and records the finished transcript.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::history::{self, Entry};
use crate::i18n::{t, tf};
use crate::recorder::{self, Recorder, Track, TrackKind, SAMPLE_RATE};
use crate::sounds::{self, Sound};
use crate::state::AppState;
use crate::{local, whisper};

/// Nothing is cut before the pending audio is this long.
const MIN_CHUNK: usize = 30 * SAMPLE_RATE as usize;
/// A cut is looked for from this point on.
const SEARCH_FROM: usize = 20 * SAMPLE_RATE as usize;
/// The pending audio is cut here at the latest (OpenAI takes 25 MB; this is ~1.9 MB).
const MAX_CHUNK: usize = 60 * SAMPLE_RATE as usize;
/// Length of the windows compared when looking for a pause (300 ms).
const WINDOW: usize = SAMPLE_RATE as usize * 3 / 10;
/// A window quieter than this is a pause worth cutting at.
const SILENCE_RMS: f32 = 0.01;
/// A chunk quieter than this is not sent to the model (it would hallucinate).
const QUIET_RMS: f32 = 0.003;
/// How often the chunker drains the recorder.
const DRAIN_INTERVAL: Duration = Duration::from_secs(1);

/// One transcribed stretch of the meeting.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Who spoke; only known when the recording has a microphone *and* a
    /// system-audio track.
    pub speaker: Option<TrackKind>,
    pub text: String,
}

/// What the UI sees of the meeting (event `meeting` and command `meeting_state`).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingView {
    pub active: bool,
    /// The recording is over and the last chunks are being transcribed.
    pub finishing: bool,
    /// Unix time in ms.
    pub started_ts: u64,
    pub model: String,
    pub segments: Vec<Segment>,
}

#[derive(Default)]
struct Inner {
    view: MeetingView,
    /// Asks the chunker to stop while the meeting is recording.
    stop: Option<Arc<AtomicBool>>,
}

/// Managed state. The last finished meeting stays in the view until the next
/// one starts.
#[derive(Default)]
pub struct MeetingState(Mutex<Inner>);

/// A slice of one track, ready to be transcribed.
struct Chunk {
    speaker: Option<TrackKind>,
    start_ms: u64,
    samples: Vec<f32>,
}

/// What the transcriber needs, fixed when the meeting starts.
struct Job {
    model: String,
    /// The local model name, or `None` for OpenAI.
    local: Option<String>,
    api_key: String,
    opts: whisper::Options,
    save_history: bool,
}

/// Root-mean-square amplitude.
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Where to cut the pending audio of a track, if it is time to.
///
/// Nothing before 30 s. After that, the quietest 300 ms window between 20 s
/// and 60 s is the candidate; it is used if it is a real pause, or if the
/// audio has reached 60 s and has to be cut somewhere.
fn cut_point(samples: &[f32]) -> Option<usize> {
    if samples.len() < MIN_CHUNK {
        return None;
    }
    let end = samples.len().min(MAX_CHUNK);
    let (start, level) = (SEARCH_FROM..=end - WINDOW)
        .step_by(WINDOW / 3)
        .map(|s| (s, rms(&samples[s..s + WINDOW])))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    (level < SILENCE_RMS || samples.len() >= MAX_CHUNK).then_some(start + WINDOW / 2)
}

/// Whether a chunk is too quiet to be worth transcribing.
fn is_quiet(samples: &[f32]) -> bool {
    rms(samples) < QUIET_RMS
}

fn ms_of(samples: usize) -> u64 {
    samples as u64 * 1000 / SAMPLE_RATE as u64
}

/// `hh:mm:ss`.
fn timestamp(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// One line per segment: `[hh:mm:ss] Me: text`, without the label when the
/// speaker is unknown.
fn format_transcript(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|seg| {
            let label = match seg.speaker {
                Some(TrackKind::Mic) => format!("{}: ", t("meeting.me")),
                Some(TrackKind::System) => format!("{}: ", t("meeting.others")),
                None => String::new(),
            };
            format!("[{}] {label}{}", timestamp(seg.start_ms), seg.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Keep `segments` ordered by start; equal starts keep their arrival order.
fn insert_segment(segments: &mut Vec<Segment>, seg: Segment) {
    let at = segments.partition_point(|s| s.start_ms <= seg.start_ms);
    segments.insert(at, seg);
}

/// Audio of a track not cut into a chunk yet.
struct Pending {
    speaker: Option<TrackKind>,
    /// Samples already cut off, i.e. where `buf` starts on the timeline.
    offset: usize,
    buf: Vec<f32>,
}

impl Pending {
    fn send_front(&mut self, len: usize, tx: &Sender<Chunk>) {
        let samples: Vec<f32> = self.buf.drain(..len).collect();
        let chunk = Chunk {
            speaker: self.speaker,
            start_ms: ms_of(self.offset),
            samples,
        };
        self.offset += len;
        let _ = tx.send(chunk);
    }

    /// Cut every chunk that is due.
    fn cut(&mut self, tx: &Sender<Chunk>) {
        while let Some(at) = cut_point(&self.buf) {
            self.send_front(at, tx);
        }
    }

    /// Send whatever is left.
    fn flush(&mut self, tx: &Sender<Chunk>) {
        if !self.buf.is_empty() {
            self.send_front(self.buf.len(), tx);
        }
    }

    /// Length of this track so far.
    fn len(&self) -> usize {
        self.offset + self.buf.len()
    }
}

/// Add freshly drained audio to the pending buffers (created on first use).
fn feed(pending: &mut Vec<Pending>, tracks: Vec<Track>, two_tracks: bool) {
    for (i, track) in tracks.into_iter().enumerate() {
        if pending.len() <= i {
            pending.push(Pending {
                speaker: two_tracks.then_some(track.kind),
                offset: 0,
                buf: Vec::new(),
            });
        }
        pending[i].buf.extend_from_slice(&track.samples);
    }
}

/// Drain the recorder until asked to stop, cutting chunks as they become due.
/// Returns the length of the recording in ms.
fn run_chunker(rec: Recorder, stop: &AtomicBool, tx: Sender<Chunk>) -> u64 {
    let two_tracks = matches!(rec.source(), recorder::Source::Mix(_));
    let mut pending: Vec<Pending> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        // Sleep in small steps so that stopping is prompt.
        let mut slept = Duration::ZERO;
        while slept < DRAIN_INTERVAL && !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100));
            slept += Duration::from_millis(100);
        }
        feed(&mut pending, rec.drain(), two_tracks);
        pending.iter_mut().for_each(|p| p.cut(&tx));
    }
    match rec.stop() {
        Ok(rest) => feed(&mut pending, rest.tracks, two_tracks),
        Err(e) => eprintln!("[whisperaround] stopping the meeting recorder: {e}"),
    }
    pending.iter_mut().for_each(|p| p.flush(&tx));
    ms_of(pending.iter().map(Pending::len).max().unwrap_or(0))
}

/// Transcribe one chunk with the model chosen when the meeting started.
fn transcribe_chunk(app: &AppHandle, job: &Job, samples: &[f32]) -> Result<String> {
    let wav = recorder::encode_wav(samples)?;
    match &job.local {
        Some(name) => local::transcribe(app, name, &wav, &job.opts),
        // This is a plain thread, not an async task.
        None => tauri::async_runtime::block_on(whisper::transcribe(&job.api_key, &job.model, wav, &job.opts)),
    }
}

/// Transcribe the chunks as they arrive, then record the finished meeting.
fn run_transcriber(app: AppHandle, rx: Receiver<Chunk>, chunker: JoinHandle<u64>, job: Job) {
    for chunk in rx {
        if is_quiet(&chunk.samples) {
            continue;
        }
        let result = transcribe_chunk(&app, &job, &chunk.samples)
            .or_else(|_| transcribe_chunk(&app, &job, &chunk.samples));
        match result {
            Ok(text) => {
                let text = text.trim().to_string();
                if !text.is_empty() {
                    let seg = Segment {
                        start_ms: chunk.start_ms,
                        end_ms: chunk.start_ms + ms_of(chunk.samples.len()),
                        speaker: chunk.speaker,
                        text,
                    };
                    update(&app, |view| insert_segment(&mut view.segments, seg));
                }
            }
            Err(e) => {
                let _ = app.emit("notice", tf("meeting.error.chunk", &[("error", &e)]));
            }
        }
    }
    let duration_ms = chunker.join().unwrap_or(0);
    finish(&app, &job, duration_ms);
}

/// Record the transcript and make the meeting inactive again.
fn finish(app: &AppHandle, job: &Job, duration_ms: u64) {
    let view = app.state::<MeetingState>().0.lock().unwrap().view.clone();
    if !view.segments.is_empty() {
        let entry = Entry {
            id: history::now_ms(),
            ts: view.started_ts,
            words: view.segments.iter().map(|s| s.text.split_whitespace().count() as u32).sum(),
            text: format_transcript(&view.segments),
            duration_ms,
            model: job.model.clone(),
            mode: t("meeting.title"),
            app: None,
            meeting: true,
        };
        history::record(app, entry, job.save_history);
    }
    {
        let state = app.state::<MeetingState>();
        let mut inner = state.0.lock().unwrap();
        inner.view.active = false;
        inner.view.finishing = false;
        inner.stop = None;
    }
    emit(app);
    refresh_tray(app);
}

/// Change the view and tell every window.
fn update(app: &AppHandle, change: impl FnOnce(&mut MeetingView)) {
    change(&mut app.state::<MeetingState>().0.lock().unwrap().view);
    emit(app);
}

fn emit(app: &AppHandle) {
    let _ = app.emit("meeting", snapshot(app));
}

/// Current view (command `meeting_state`).
pub fn snapshot(app: &AppHandle) -> MeetingView {
    app.state::<MeetingState>().0.lock().unwrap().view.clone()
}

/// Whether a meeting is being recorded (not just finishing).
pub fn recording(app: &AppHandle) -> bool {
    let view = snapshot(app);
    view.active && !view.finishing
}

/// Rebuild the tray on the main thread, so its label and tooltip follow the state.
fn refresh_tray(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || crate::refresh_tray(&handle));
}

/// Start a meeting, or stop the running one.
pub fn toggle(app: &AppHandle) {
    let (active, finishing) = {
        let view = snapshot(app);
        (view.active, view.finishing)
    };
    match (active, finishing) {
        (false, _) => start(app),
        (true, false) => stop(app),
        (true, true) => {}
    }
}

/// Start recording a meeting. Without the user's consent the settings open
/// on the Meetings page instead.
pub fn start(app: &AppHandle) {
    let state = app.state::<MeetingState>();
    let mut inner = state.0.lock().unwrap();
    if inner.view.active {
        return;
    }
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    if !settings.meeting_consent {
        drop(inner);
        crate::commands::show_settings_window(app);
        let _ = app.emit("meeting-consent", ());
        return;
    }

    // Without a key every chunk would fail; say it once instead.
    let model = settings.effective_model().to_string();
    if local::model_name(&model).is_none() && settings.openai_api_key.trim().is_empty() {
        let _ = app.emit("notice", t("error.openai.noKey"));
        return;
    }

    sounds::play(app, Sound::Start);
    let rec = match Recorder::start(&settings.input_device, |_| {}) {
        Ok(rec) => rec,
        Err(e) => {
            let key = if matches!(recorder::Source::parse(&settings.input_device), recorder::Source::Mic(_)) {
                "error.mic.open"
            } else {
                "error.recording.failed"
            };
            let _ = app.emit("notice", tf(key, &[("error", &e)]));
            return;
        }
    };

    let job = Job {
        local: local::model_name(&model).map(String::from),
        model: model.clone(),
        api_key: settings.openai_api_key.clone(),
        opts: whisper::Options {
            language: settings.effective_language().to_string(),
            prompt: settings.vocabulary_prompt(),
        },
        save_history: settings.save_history,
    };
    let stop = Arc::new(AtomicBool::new(false));
    inner.stop = Some(stop.clone());
    inner.view = MeetingView {
        active: true,
        finishing: false,
        started_ts: history::now_ms(),
        model,
        segments: Vec::new(),
    };
    drop(inner);

    let (tx, rx) = mpsc::channel();
    let chunker = thread::spawn(move || run_chunker(rec, &stop, tx));
    let handle = app.clone();
    thread::spawn(move || run_transcriber(handle, rx, chunker, job));
    emit(app);
    refresh_tray(app);
}

/// Stop recording; the transcriber finishes the queue and saves the meeting.
pub fn stop(app: &AppHandle) {
    {
        let state = app.state::<MeetingState>();
        let mut inner = state.0.lock().unwrap();
        let Some(stop) = inner.stop.clone().filter(|_| !inner.view.finishing) else {
            return;
        };
        inner.view.finishing = true;
        stop.store(true, Ordering::Relaxed);
    }
    sounds::play(app, Sound::Stop);
    emit(app);
    refresh_tray(app);
}

/// Keep only the base name, replace what Windows or Unix file systems reject
/// and force a `.md` extension.
fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base
        .chars()
        .map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let clean = clean.trim().trim_matches('.');
    let stem = clean.strip_suffix(".md").or_else(|| clean.strip_suffix(".MD")).unwrap_or(clean);
    let stem = if stem.trim().is_empty() { "Meeting" } else { stem };
    format!("{stem}.md")
}

/// `<Documents>/WhisperAround`.
fn export_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .document_dir()
        .map(|d| d.join("WhisperAround"))
        .map_err(|e| tf("meeting.error.export", &[("error", &e)]))
}

/// Save `content` as `file_name` in the export folder, open it with the
/// default app and return its path.
pub fn export(app: &AppHandle, file_name: &str, content: &str) -> Result<String, String> {
    let fail = |e: &dyn std::fmt::Display| tf("meeting.error.export", &[("error", e)]);
    let dir = export_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| fail(&e))?;
    let path = dir.join(sanitize_file_name(file_name));
    std::fs::write(&path, content).map_err(|e| fail(&e))?;
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(program).arg(&path).spawn() {
        eprintln!("[whisperaround] opening the exported meeting: {e}");
    }
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: usize = SAMPLE_RATE as usize;

    /// `secs` seconds of a loud tone.
    fn loud(secs: usize) -> Vec<f32> {
        (0..secs * SR).map(|i| if i % 2 == 0 { 0.3 } else { -0.3 }).collect()
    }

    #[test]
    fn nothing_is_cut_before_thirty_seconds() {
        assert_eq!(cut_point(&loud(29)), None);
        assert_eq!(cut_point(&[]), None);
    }

    #[test]
    fn cuts_at_a_pause() {
        let mut audio = loud(40);
        // One second of silence from 25 s.
        audio[25 * SR..26 * SR].fill(0.0);
        let at = cut_point(&audio).unwrap();
        assert!((25 * SR..=26 * SR).contains(&at), "{at}");
    }

    #[test]
    fn keeps_waiting_without_a_pause_until_sixty_seconds() {
        assert_eq!(cut_point(&loud(45)), None);
        assert_eq!(cut_point(&loud(59)), None);
        let at = cut_point(&loud(75)).unwrap();
        assert!((SEARCH_FROM..=MAX_CHUNK).contains(&at), "{at}");
    }

    #[test]
    fn a_pause_before_twenty_seconds_is_ignored() {
        let mut audio = loud(40);
        audio[5 * SR..8 * SR].fill(0.0);
        assert_eq!(cut_point(&audio), None);
    }

    #[test]
    fn quiet_chunks_are_detected() {
        assert!(is_quiet(&[]));
        assert!(is_quiet(&vec![0.0; 1000]));
        assert!(is_quiet(&vec![0.002; 1000]));
        assert!(!is_quiet(&vec![0.05; 1000]));
    }

    fn seg(start_ms: u64, speaker: Option<TrackKind>, text: &str) -> Segment {
        Segment {
            start_ms,
            end_ms: start_ms + 1000,
            speaker,
            text: text.into(),
        }
    }

    #[test]
    fn transcript_has_timestamps_and_labels() {
        let segments = [
            seg(0, Some(TrackKind::Mic), "Hello"),
            seg(3_725_000, Some(TrackKind::System), "Hi there"),
        ];
        assert_eq!(format_transcript(&segments), "[00:00:00] Me: Hello\n[01:02:05] Others: Hi there");
        assert_eq!(format_transcript(&[seg(61_000, None, "Solo")]), "[00:01:01] Solo");
    }

    #[test]
    fn segments_stay_ordered_by_start() {
        let mut v = Vec::new();
        insert_segment(&mut v, seg(30_000, None, "b"));
        insert_segment(&mut v, seg(0, None, "a"));
        insert_segment(&mut v, seg(30_000, None, "c"));
        let texts: Vec<_> = v.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["a", "b", "c"]);
    }

    #[test]
    fn chunks_know_where_they_start() {
        let (tx, rx) = mpsc::channel();
        let mut p = Pending {
            speaker: None,
            offset: 0,
            buf: loud(75),
        };
        p.cut(&tx);
        p.flush(&tx);
        let chunks: Vec<Chunk> = rx.try_iter().collect();
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[0].start_ms, 0);
        assert_eq!(chunks[1].start_ms, ms_of(chunks[0].samples.len()));
        assert_eq!(chunks.iter().map(|c| c.samples.len()).sum::<usize>(), 75 * SR);
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(sanitize_file_name("2026-10-21 09-30 Meeting.md"), "2026-10-21 09-30 Meeting.md");
        assert_eq!(sanitize_file_name("notes"), "notes.md");
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd.md");
        assert_eq!(sanitize_file_name("C:\\Users\\me\\a:b*c?.txt"), "a_b_c_.txt.md");
        assert_eq!(sanitize_file_name("a\"b<c>d|e\u{7}f"), "a_b_c_d_e_f.md");
        assert_eq!(sanitize_file_name("  "), "Meeting.md");
        assert_eq!(sanitize_file_name("dir/"), "Meeting.md");
    }
}
