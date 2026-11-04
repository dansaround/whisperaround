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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::echo::EchoCanceller;
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
    /// Save the transcript as Markdown in the meetings folder when it ends.
    autosave: bool,
    /// File name settings, copied when the meeting starts.
    date_format: String,
    name: String,
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

/// Raw and processed audio of a mix, kept to debug the echo suppression.
#[derive(Default)]
struct Dump {
    mic_raw: Vec<f32>,
    system: Vec<f32>,
    mic_aec: Vec<f32>,
}

/// Keeps the speakers out of the microphone track of a mix. Echo suppression
/// needs both tracks on one timeline, so it runs on what is drained, before
/// the tracks are cut into chunks one by one.
struct EchoFilter {
    /// `None` when the echo suppression is off: the microphone passes as is.
    canceller: Option<EchoCanceller>,
    /// Stream generations of the microphone and the system track when last fed.
    generations: Option<(u64, u64)>,
    dump: Option<Dump>,
}

impl EchoFilter {
    fn new(cancel: bool, dump: bool) -> Self {
        Self {
            canceller: cancel.then(EchoCanceller::new),
            generations: None,
            dump: dump.then(Dump::default),
        }
    }

    /// Replace the microphone samples of freshly drained `tracks` by the ones
    /// that are ready; the rest follows with later drains and [`Self::finish`].
    fn apply(&mut self, tracks: &mut [Track]) {
        let position = |kind| tracks.iter().position(|t| t.kind == kind);
        let (Some(mic), Some(system)) = (position(TrackKind::Mic), position(TrackKind::System)) else {
            return;
        };
        // A new stream may have a different echo path (and delay).
        let generations = (tracks[mic].generation, tracks[system].generation);
        if let Some(canceller) = self.canceller.as_mut().filter(|_| self.generations.is_some_and(|g| g != generations)) {
            canceller.reset();
        }
        self.generations = Some(generations);

        let raw = std::mem::take(&mut tracks[mic].samples);
        let ready = match self.canceller.as_mut() {
            Some(canceller) => canceller.process(&raw, &tracks[system].samples),
            None => raw.clone(),
        };
        if let Some(dump) = self.dump.as_mut() {
            dump.mic_raw.extend_from_slice(&raw);
            dump.system.extend_from_slice(&tracks[system].samples);
            dump.mic_aec.extend_from_slice(&ready);
        }
        tracks[mic].samples = ready;
    }

    /// The microphone samples still held back, once the recording is over.
    fn finish(&mut self) -> Vec<f32> {
        let rest = self.canceller.as_mut().map(EchoCanceller::flush).unwrap_or_default();
        if let Some(dump) = self.dump.as_mut() {
            dump.mic_aec.extend_from_slice(&rest);
        }
        rest
    }
}

/// Add freshly drained audio to the pending buffers (created on first use).
fn feed(pending: &mut Vec<Pending>, mut tracks: Vec<Track>, two_tracks: bool, echo: &mut EchoFilter) {
    if two_tracks {
        echo.apply(&mut tracks);
    }
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

/// What the chunker leaves behind when the recording ends.
#[derive(Default)]
struct Recorded {
    duration_ms: u64,
    dump: Option<Dump>,
}

/// Drain the recorder until asked to stop, cutting chunks as they become due.
fn run_chunker(rec: Recorder, stop: &AtomicBool, tx: Sender<Chunk>, mut echo: EchoFilter) -> Recorded {
    let two_tracks = matches!(rec.source(), recorder::Source::Mix(_));
    let mut pending: Vec<Pending> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        // Sleep in small steps so that stopping is prompt.
        let mut slept = Duration::ZERO;
        while slept < DRAIN_INTERVAL && !stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100));
            slept += Duration::from_millis(100);
        }
        feed(&mut pending, rec.drain(), two_tracks, &mut echo);
        pending.iter_mut().for_each(|p| p.cut(&tx));
    }
    match rec.stop() {
        Ok(rest) => feed(&mut pending, rest.tracks, two_tracks, &mut echo),
        Err(e) => eprintln!("[whisperaround] stopping the meeting recorder: {e}"),
    }
    if let Some(mic) = pending.iter_mut().find(|p| p.speaker == Some(TrackKind::Mic)) {
        mic.buf.extend(echo.finish());
    }
    pending.iter_mut().for_each(|p| p.flush(&tx));
    Recorded {
        duration_ms: ms_of(pending.iter().map(Pending::len).max().unwrap_or(0)),
        dump: echo.dump,
    }
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
fn run_transcriber(app: AppHandle, rx: Receiver<Chunk>, chunker: JoinHandle<Recorded>, job: Job) {
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
    let recorded = chunker.join().unwrap_or_default();
    finish(&app, &job, recorded);
}

/// Record the transcript and make the meeting inactive again.
fn finish(app: &AppHandle, job: &Job, recorded: Recorded) {
    let Recorded { duration_ms, dump } = recorded;
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
        if job.autosave {
            autosave(app, &view, duration_ms, &job.date_format, &job.name, dump.as_ref());
        }
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
    let source = settings.meeting_recorder_source();
    let rec = match Recorder::start(&source, |_| {}) {
        Ok(rec) => rec,
        Err(e) => {
            let key = if matches!(recorder::Source::parse(&source), recorder::Source::Mic(_)) {
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
        autosave: settings.meeting_autosave,
        date_format: settings.meeting_date_format.clone(),
        name: settings.meeting_name.clone(),
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

    let echo = EchoFilter::new(settings.echo_cancel, settings.meeting_audio_dump);
    let (tx, rx) = mpsc::channel();
    let chunker = thread::spawn(move || run_chunker(rec, &stop, tx, echo));
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

/// The folder chosen in the settings, or `<Documents>/WhisperAround`.
pub fn export_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let chosen = app.state::<AppState>().settings.lock().unwrap().meeting_folder.clone();
    if !chosen.is_empty() {
        return Ok(PathBuf::from(chosen));
    }
    app.path()
        .document_dir()
        .map(|d| d.join("WhisperAround"))
        .map_err(|e| tf("meeting.error.export", &[("error", &e)]))
}

/// Open a file or folder with the default app.
pub fn open_path(path: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(program).arg(path).spawn() {
        eprintln!("[whisperaround] opening {}: {e}", path.display());
    }
}

/// Write `content` as `file_name` in the meetings folder and return its path.
fn save(app: &AppHandle, file_name: &str, content: &str) -> Result<PathBuf, String> {
    let fail = |e: &dyn std::fmt::Display| tf("meeting.error.export", &[("error", e)]);
    let dir = export_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| fail(&e))?;
    let path = dir.join(sanitize_file_name(file_name));
    std::fs::write(&path, content).map_err(|e| fail(&e))?;
    Ok(path)
}

/// Save `content` as `file_name` in the meetings folder, open it with the
/// default app and return its path.
pub fn export(app: &AppHandle, file_name: &str, content: &str) -> Result<String, String> {
    let path = save(app, file_name, content)?;
    open_path(&path);
    Ok(path.to_string_lossy().into_owned())
}

/// Local time of a Unix timestamp in ms.
fn local_time(ts_ms: u64) -> chrono::DateTime<chrono::Local> {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ts_ms as i64)
        .single()
        .unwrap_or_else(chrono::Local::now)
}

/// `mm:ss`, or `h:mm:ss` from one hour on (like `clock` in `src/lib/meeting.ts`).
fn clock(ms: u64) -> String {
    let s = ms / 1000;
    match s / 3600 {
        0 => format!("{:02}:{:02}", s / 60, s % 60),
        h => format!("{h}:{:02}:{:02}", s / 60 % 60, s % 60),
    }
}

/// The Markdown file of a finished meeting; mirrors `meetingMarkdown` and
/// `meetingFileName` in `src/lib/meeting.ts`, which the Export button uses.
fn markdown(view: &MeetingView, duration_ms: u64, date_format: &str, name: &str) -> (String, String) {
    let time = local_time(view.started_ts);
    let name = match name.trim() {
        "" => t("meeting.md.name"),
        custom => custom.to_string(),
    };
    let date = date_text(&time, date_format);
    let file_name = format!("{date} {} {name}.md", time.format("%H-%M-%S"));
    // Blank lines between segments, or Markdown would run them into one paragraph.
    let transcript = format_transcript(&view.segments).replace('\n', "\n\n");
    let content = [
        format!("# {date} {name} {}", time.format("%H:%M")),
        String::new(),
        format!("- {}", tf("meeting.md.duration", &[("time", &clock(duration_ms))])),
        format!("- {}", tf("meeting.md.model", &[("model", &view.model)])),
        String::new(),
        transcript,
        String::new(),
    ]
    .join("\n");
    (file_name, content)
}

/// The date in a meeting's file name, in the order the settings choose.
fn date_text(time: &chrono::DateTime<chrono::Local>, format: &str) -> String {
    match format {
        "dmy" => time.format("%d-%m-%Y").to_string(),
        "mdy" => time.format("%m-%d-%Y").to_string(),
        _ => time.format("%Y-%m-%d").to_string(),
    }
}

/// Write the audio of `dump` next to the transcript at `path`, as
/// `<name>.mic_raw.wav`, `<name>.system.wav` and `<name>.mic_aec.wav`.
fn save_dump(path: &Path, dump: &Dump) {
    for (suffix, samples) in [("mic_raw", &dump.mic_raw), ("system", &dump.system), ("mic_aec", &dump.mic_aec)] {
        let target = path.with_extension(format!("{suffix}.wav"));
        let written = recorder::encode_wav(samples).map_err(|e| e.to_string()).and_then(|wav| std::fs::write(&target, wav).map_err(|e| e.to_string()));
        if let Err(e) = written {
            eprintln!("[whisperaround] saving {}: {e}", target.display());
        }
    }
}

/// Save a finished meeting in the meetings folder and say where.
fn autosave(app: &AppHandle, view: &MeetingView, duration_ms: u64, date_format: &str, name: &str, dump: Option<&Dump>) {
    let (file_name, content) = markdown(view, duration_ms, date_format, name);
    let notice = match save(app, &file_name, &content) {
        Ok(path) => {
            if let Some(dump) = dump {
                save_dump(&path, dump);
            }
            tf("meeting.saved", &[("path", &path.display())])
        }
        Err(e) => e,
    };
    let _ = app.emit("notice", notice);
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

    /// Both tracks of a mix, `n` samples each: the microphone at `level`, the
    /// system silent (so there is no echo), in stream `generation`.
    fn mix(n: usize, level: f32, generation: u64) -> Vec<Track> {
        [(TrackKind::Mic, level), (TrackKind::System, 0.0)]
            .map(|(kind, level)| Track { kind, samples: vec![level; n], generation })
            .into()
    }

    #[test]
    fn feeding_through_the_echo_filter_keeps_lengths_and_offsets() {
        let mut echo = EchoFilter::new(true, true);
        let mut pending = Vec::new();
        let drains = [15_900, 16_100, 16_000, 7_777, 16_000];
        for (i, n) in drains.into_iter().enumerate() {
            // The stream of the microphone is reopened halfway.
            feed(&mut pending, mix(n, 0.2, 1 + (i >= 2) as u64), true, &mut echo);
        }
        let total: usize = drains.iter().sum();
        let (tx, rx) = mpsc::channel();
        pending[0].buf.extend(echo.finish());
        // Held back by the lookahead, never ahead of the input.
        assert!(pending[0].len() <= total);
        assert_eq!(pending[1].len(), total);
        pending.iter_mut().for_each(|p| p.flush(&tx));
        assert_eq!(pending[0].len(), total);
        assert_eq!(pending[0].speaker, Some(TrackKind::Mic));
        assert_eq!(pending[1].speaker, Some(TrackKind::System));
        // Nothing plays on the system, so the microphone is unchanged.
        let chunks: Vec<Chunk> = rx.try_iter().collect();
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|c| c.start_ms == 0 && c.samples.len() == total));
        assert!(chunks[0].samples.iter().all(|&s| s == 0.2));
        let dump = echo.dump.unwrap();
        assert_eq!((dump.mic_raw.len(), dump.system.len(), dump.mic_aec.len()), (total, total, total));
    }

    #[test]
    fn feeding_without_the_filter_or_a_mix_leaves_tracks_alone() {
        let mut pending = Vec::new();
        let mut off = EchoFilter::new(false, false);
        feed(&mut pending, mix(1_000, 0.1, 1), true, &mut off);
        assert_eq!((pending[0].len(), pending[1].len()), (1_000, 1_000));
        assert!(off.finish().is_empty());

        let mut single = Vec::new();
        let mut on = EchoFilter::new(true, false);
        let mic = vec![Track { kind: TrackKind::Mic, samples: vec![0.1; 500], generation: 1 }];
        feed(&mut single, mic, false, &mut on);
        assert_eq!(single[0].len(), 500);
        assert_eq!(single[0].speaker, None);
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

    #[test]
    fn markdown_has_a_heading_and_one_paragraph_per_segment() {
        let view = MeetingView {
            started_ts: 1_700_000_000_000,
            model: "local:small".into(),
            segments: vec![
                Segment { start_ms: 0, end_ms: 1_000, speaker: None, text: "Hola".into() },
                Segment { start_ms: 65_000, end_ms: 70_000, speaker: None, text: "Chao".into() },
            ],
            ..Default::default()
        };
        let (file_name, content) = markdown(&view, 70_000, "ymd", "");
        assert!(file_name.ends_with(".md"));
        assert!(content.starts_with("# "));
        assert!(content.contains("01:10"));
        assert!(content.contains("[00:00:00] Hola\n\n[00:01:05] Chao"));
    }

    #[test]
    fn file_name_has_seconds_and_the_chosen_date_and_name() {
        let view = MeetingView { started_ts: 1_700_000_000_000, ..Default::default() };
        let (file_name, content) = markdown(&view, 0, "dmy", " Team sync ");
        assert!(file_name.ends_with(" Team sync.md"), "{file_name}");
        let stem = file_name.trim_end_matches(" Team sync.md");
        let parts: Vec<&str> = stem.split(' ').collect();
        assert_eq!(parts.len(), 2, "{stem}");
        assert_eq!(parts[0].split('-').map(str::len).collect::<Vec<_>>(), [2, 2, 4]);
        assert_eq!(parts[1].split('-').map(str::len).collect::<Vec<_>>(), [2, 2, 2]);
        assert!(content.starts_with("# "));
        assert!(content.contains(" Team sync "));
    }

    #[test]
    fn a_blank_name_falls_back_to_the_translated_default() {
        let view = MeetingView { started_ts: 1_700_000_000_000, ..Default::default() };
        let (file_name, _) = markdown(&view, 0, "ymd", "   ");
        assert!(file_name.ends_with(&format!(" {}.md", t("meeting.md.name"))), "{file_name}");
    }

    #[test]
    fn clock_switches_to_hours() {
        assert_eq!(clock(65_000), "01:05");
        assert_eq!(clock(3_725_000), "1:02:05");
    }
}

