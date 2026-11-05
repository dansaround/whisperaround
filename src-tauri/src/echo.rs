//! Offline echo suppression for the two tracks of a meeting.
//!
//! Without headphones the microphone ('Me') picks up the speakers, so Whisper
//! would transcribe the system audio ('Others') twice. Audio quality does not
//! matter here, only that the transcript stays clean, so instead of a
//! subtractive canceller this is an aligned gate: it finds how much later the
//! system audio shows up in the microphone, learns per frequency band how
//! strongly it couples in, and silences the microphone while its energy is
//! explained by that coupling. Near-end speech is louder than the predicted
//! echo and passes untouched, and with headphones there is no coupling, so the
//! output is the input.
//!
//! Both tracks share one timeline (the same sample index is about the same
//! wall time), so the canceller works on absolute sample positions. It emits a
//! microphone sample only once the system audio up to [`LOOKAHEAD`] later has
//! arrived, because jitter and clock offsets can put the echo slightly before
//! its source. [`EchoCanceller::flush`] releases the rest, so the output is as
//! long as the input.

use std::collections::VecDeque;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::recorder::SAMPLE_RATE;

/// Samples per decision (20 ms).
const HOP: usize = 320;
/// Analysis window of a decision, ending with its hop.
const FRAME: usize = 512;
/// Bins of each frequency band, as `[start, end)`; the coupling is learned per band.
const BANDS: usize = 16;
const BAND_EDGES: [usize; BANDS + 1] = [1, 4, 8, 12, 16, 20, 24, 32, 40, 48, 56, 64, 80, 96, 128, 160, 256];

/// How far the echo may come before its source (150 ms).
const LOOKAHEAD: usize = SAMPLE_RATE as usize * 3 / 20;
/// How far the echo may lag its source (500 ms; Bluetooth adds 150-300 ms).
const MAX_LAG: usize = SAMPLE_RATE as usize / 2;
/// Earlier frames of the system audio that still count as echo (reverb).
const TAIL_FRAMES: usize = 10;
/// How much each earlier tail frame weighs less than the next one.
const TAIL_DECAY: f32 = 0.8;

/// Window of the delay estimate (2 s) and how often it is redone (1 s; twice
/// as often while there is no delay).
const ESTIMATE_WINDOW: usize = 2 * SAMPLE_RATE as usize;
const ESTIMATE_EVERY: usize = SAMPLE_RATE as usize;
/// Shortest window worth estimating on.
const ESTIMATE_MIN: usize = SAMPLE_RATE as usize;
/// The system audio must be at least this loud (mean square) for an estimate.
const ESTIMATE_MIN_POWER: f32 = 1e-5;
/// Peak of the correlation over its typical level that counts as an echo.
const MIN_CONFIDENCE: f32 = 8.0;
/// A new delay this far from the current one is a jump (50 ms); it needs two
/// agreeing estimates in a row.
const JUMP: isize = SAMPLE_RATE as isize / 20;
/// Estimates this close are the same jump (25 ms).
const JUMP_AGREE: isize = SAMPLE_RATE as isize / 40;
/// Frequencies the delay is estimated on.
const ESTIMATE_HZ: (usize, usize) = (200, 3_800);

/// The system audio (with its tail) is active above this mean square.
const REF_ACTIVE: f32 = 5e-6;
/// Predicted echo over system audio power below which there is no coupling.
const MIN_COUPLING: f32 = 0.002;
/// The microphone is silenced while it is at most this many times the
/// predicted echo, or while at most [`UNEXPLAINED`] of it is not explained by
/// the echo band by band (up to [`GATE_RATIO`] times the predicted power of
/// each); near-end speech does not have the spectrum of the system audio.
const TOTAL_RATIO: f32 = 2.0;
const GATE_RATIO: f32 = 4.0;
const UNEXPLAINED: f32 = 0.1;
/// Smoothing of the band powers the coupling is the ratio of.
const POWER_SMOOTHING: f32 = 0.1;
/// The coupling is a low quantile of the microphone over system power seen in
/// the last 3 s of frames with system audio. Echo alone gives the coupling
/// itself and near-end speech only larger ratios, and speech has pauses, so the
/// quantile follows a new path within a second or two and cannot lock on a
/// wrong value, which an estimate that learns only from quiet frames can.
const HISTORY_FRAMES: usize = 150;
const COUPLING_QUANTILE: f32 = 0.25;
/// Ratios seen before the coupling is trusted (0.5 s).
const MIN_HISTORY: usize = 25;
/// Largest microphone over system power of a band.
const MAX_RATIO: f32 = 10.0;
/// Samples a change of gate is faded over (5 ms).
const RAMP: usize = SAMPLE_RATE as usize / 200;

/// Power per frequency band.
type Bands = [f32; BANDS];

/// Samples of `buf`, whose first sample is number `base` on the timeline;
/// silence outside of it.
fn sample_at(buf: &[f32], base: usize, i: isize) -> f32 {
    let at = i - base as isize;
    if at < 0 {
        return 0.0;
    }
    buf.get(at as usize).copied().unwrap_or(0.0)
}

/// Band powers of frames of [`FRAME`] samples.
struct Spectrum {
    plan: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    /// Turns `|X|^2` of a bin into mean square (both halves of the spectrum).
    scale: f32,
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
}

impl Spectrum {
    fn new(planner: &mut RealFftPlanner<f32>) -> Self {
        let plan = planner.plan_fft_forward(FRAME);
        let window: Vec<f32> = (0..FRAME)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FRAME as f32).cos())
            .collect();
        let energy: f32 = window.iter().map(|w| w * w).sum();
        Self {
            input: plan.make_input_vec(),
            output: plan.make_output_vec(),
            scale: 2.0 / (FRAME as f32 * energy),
            plan,
            window,
        }
    }

    /// Band powers (mean square) of the frame that ends right before `end`.
    fn bands(&mut self, signal: impl Fn(isize) -> f32, end: isize) -> Bands {
        let start = end - FRAME as isize;
        let mut any = false;
        for (i, x) in self.input.iter_mut().enumerate() {
            let s = signal(start + i as isize);
            any |= s != 0.0;
            *x = s * self.window[i];
        }
        let mut bands = [0.0; BANDS];
        if !any {
            return bands;
        }
        self.plan.process(&mut self.input, &mut self.output).expect("fft sizes match");
        for (b, edge) in BAND_EDGES.windows(2).enumerate() {
            bands[b] = self.output[edge[0]..edge[1]].iter().map(|c| c.norm_sqr()).sum::<f32>() * self.scale;
        }
        bands
    }
}

/// What the canceller has learned about the echo path. Dropped whenever the
/// path may have changed.
#[derive(Default)]
struct Coupling {
    /// Smoothed microphone and (tailed) system power.
    mic: Bands,
    reference: Bands,
    /// Smoothed frames seen, with system audio.
    frames: u32,
    /// Microphone over system power per band, of the latest frames.
    history: VecDeque<Bands>,
}

impl Coupling {
    /// Take a frame with system audio in.
    fn push(&mut self, mic: &Bands, reference: &Bands) {
        let k = if self.frames == 0 { 1.0 } else { POWER_SMOOTHING };
        let mut ratio = [0.0; BANDS];
        for b in 0..BANDS {
            self.mic[b] += k * (mic[b] - self.mic[b]);
            self.reference[b] += k * (reference[b] - self.reference[b]);
            ratio[b] = (self.mic[b] / self.reference[b].max(1e-10)).min(MAX_RATIO);
        }
        self.frames += 1;
        if self.history.len() == HISTORY_FRAMES {
            self.history.pop_front();
        }
        self.history.push_back(ratio);
    }

    /// Microphone over system power per band, if there is enough to tell.
    fn ratio(&self) -> Option<Bands> {
        if self.history.len() < MIN_HISTORY {
            return None;
        }
        let nth = ((self.history.len() - 1) as f32 * COUPLING_QUANTILE) as usize;
        let mut ratio = [0.0; BANDS];
        let mut column = [0.0f32; HISTORY_FRAMES];
        let column = &mut column[..self.history.len()];
        for (b, r) in ratio.iter_mut().enumerate() {
            for (c, frame) in column.iter_mut().zip(&self.history) {
                *c = frame[b];
            }
            *r = *column.select_nth_unstable_by(nth, f32::total_cmp).1;
        }
        Some(ratio)
    }
}

pub struct EchoCanceller {
    spectrum: Spectrum,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    /// Microphone samples, from number `mic_base` on the timeline: the ones
    /// not emitted yet and the last stretch for the delay estimate.
    mic: Vec<f32>,
    mic_base: usize,
    /// System samples, from `sys_base`.
    sys: Vec<f32>,
    sys_base: usize,
    /// Microphone samples emitted so far.
    emitted: usize,
    /// Samples by which the microphone lags the system audio.
    delay: Option<isize>,
    /// A jump of the delay waiting for a second estimate.
    jump: Option<isize>,
    next_estimate: usize,
    /// Where the echo path was last forgotten: the estimate looks no further back.
    forgotten: usize,
    coupling: Coupling,
    /// Whether the previous decision silenced the microphone.
    gated: bool,
    /// Band powers of earlier system frames, by where their window ends.
    tail: Vec<(isize, Bands)>,
}

impl EchoCanceller {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::new();
        let size = estimate_size();
        Self {
            spectrum: Spectrum::new(&mut planner),
            forward: planner.plan_fft_forward(size),
            inverse: planner.plan_fft_inverse(size),
            mic: Vec::new(),
            mic_base: 0,
            sys: Vec::new(),
            sys_base: 0,
            emitted: 0,
            delay: None,
            jump: None,
            next_estimate: ESTIMATE_MIN,
            forgotten: 0,
            coupling: Coupling::default(),
            gated: false,
            tail: Vec::new(),
        }
    }

    /// Forget the echo path (a device changed). The audio fed so far and the
    /// position on the timeline stay.
    pub fn reset(&mut self) {
        self.delay = None;
        self.jump = None;
        self.forgotten = self.emitted;
        self.next_estimate = self.emitted + ESTIMATE_MIN;
        self.coupling = Coupling::default();
        self.tail.clear();
    }

    /// Add the next stretch of both tracks and get the microphone samples that
    /// are ready, which trail the input by the lookahead.
    pub fn process(&mut self, mic: &[f32], system: &[f32]) -> Vec<f32> {
        self.mic.extend_from_slice(mic);
        self.sys.extend_from_slice(system);
        self.run(false)
    }

    /// The rest of the microphone, once no more audio will come.
    pub fn flush(&mut self) -> Vec<f32> {
        self.run(true)
    }

    fn mic_end(&self) -> usize {
        self.mic_base + self.mic.len()
    }

    fn sys_end(&self) -> usize {
        self.sys_base + self.sys.len()
    }

    fn run(&mut self, flush: bool) -> Vec<f32> {
        let mut out = Vec::new();
        while self.emitted < self.mic_end() {
            let end = self.emitted + HOP;
            if !flush && (self.mic_end() < end || self.sys_end() < end + LOOKAHEAD) {
                break;
            }
            if self.emitted >= self.next_estimate {
                self.estimate();
                // Until there is a delay, try again sooner.
                let every = if self.delay.is_some() { ESTIMATE_EVERY } else { ESTIMATE_EVERY / 2 };
                self.next_estimate = self.emitted + every;
            }
            let gate = self.decide(end as isize);
            let len = end.min(self.mic_end()) - self.emitted;
            let (from, to) = (if self.gated { 0.0 } else { 1.0 }, if gate { 0.0 } else { 1.0 });
            for i in 0..len {
                let fade = ((i + 1) as f32 / RAMP as f32).min(1.0);
                let gain = from + (to - from) * fade;
                out.push(self.mic[self.emitted - self.mic_base + i] * gain);
            }
            self.gated = gate;
            self.emitted += len;
        }
        self.trim();
        out
    }

    /// Drop the audio nothing looks at any more.
    fn trim(&mut self) {
        let keep = |base: usize, len: usize, from: usize| (from.saturating_sub(base)).min(len);
        let mic_cut = keep(self.mic_base, self.mic.len(), self.emitted.saturating_sub(ESTIMATE_WINDOW + FRAME));
        self.mic.drain(..mic_cut);
        self.mic_base += mic_cut;
        let sys_cut = keep(
            self.sys_base,
            self.sys.len(),
            self.emitted.saturating_sub(ESTIMATE_WINDOW + MAX_LAG + FRAME * 2),
        );
        self.sys.drain(..sys_cut);
        self.sys_base += sys_cut;
    }

    /// Whether the microphone is just echo in the frame that ends at `end`.
    fn decide(&mut self, end: isize) -> bool {
        let Some(delay) = self.delay else {
            return false;
        };
        let (mic, mic_base) = (&self.mic, self.mic_base);
        let mic_bands = self.spectrum.bands(|i| sample_at(mic, mic_base, i), end);

        // The system audio as it reaches the microphone now, plus its tail.
        let mut wanted = [(0isize, [0.0; BANDS]); TAIL_FRAMES + 1];
        let mut tail: Bands = [0.0; BANDS];
        let mut weight = 1.0;
        for (j, slot) in wanted.iter_mut().enumerate() {
            let at = end - (j * HOP) as isize - delay;
            let bands = match self.tail.iter().find(|(key, _)| *key == at) {
                Some((_, bands)) => *bands,
                None => {
                    let (sys, sys_base) = (&self.sys, self.sys_base);
                    self.spectrum.bands(|i| sample_at(sys, sys_base, i), at)
                }
            };
            *slot = (at, bands);
            for (t, p) in tail.iter_mut().zip(bands) {
                *t = t.max(weight * p);
            }
            weight *= TAIL_DECAY;
        }
        self.tail = wanted.to_vec();

        let mic_power: f32 = mic_bands.iter().sum();
        let ref_power: f32 = tail.iter().sum();
        if ref_power < REF_ACTIVE {
            return false;
        }

        let c = &mut self.coupling;
        c.push(&mic_bands, &tail);
        let Some(ratio) = c.ratio() else {
            return false;
        };

        let predicted: f32 = ratio.iter().zip(&tail).map(|(h, t)| h * t).sum();
        // The part of the microphone that no band's predicted echo explains.
        let unexplained: f32 = mic_bands.iter().zip(&ratio).zip(&tail).map(|((m, h), t)| (m - GATE_RATIO * h * t).max(0.0)).sum();
        predicted >= MIN_COUPLING * ref_power && (mic_power <= TOTAL_RATIO * predicted || unexplained <= UNEXPLAINED * mic_power)
    }

    /// Look for the delay between the system audio and the microphone over the
    /// last couple of seconds (GCC-PHAT) and take it if it is convincing.
    fn estimate(&mut self) {
        let len = (self.emitted - self.forgotten).min(ESTIMATE_WINDOW);
        if len < ESTIMATE_MIN {
            return;
        }
        let start = (self.emitted - len) as isize;
        let (mic, mic_base) = (&self.mic, self.mic_base);
        let (sys, sys_base) = (&self.sys, self.sys_base);
        let ref_start = start - MAX_LAG as isize;
        let ref_len = len + MAX_LAG + LOOKAHEAD;

        let power = (0..len as isize).map(|i| sample_at(sys, sys_base, start + i).powi(2)).sum::<f32>() / len as f32;
        if power < ESTIMATE_MIN_POWER {
            return;
        }

        let size = estimate_size();
        let mut a = vec![0.0f32; size];
        let mut b = vec![0.0f32; size];
        for (i, x) in a.iter_mut().take(len).enumerate() {
            *x = sample_at(mic, mic_base, start + i as isize);
        }
        for (i, x) in b.iter_mut().take(ref_len).enumerate() {
            *x = sample_at(sys, sys_base, ref_start + i as isize);
        }
        let mut mic_spec = self.forward.make_output_vec();
        let mut ref_spec = self.forward.make_output_vec();
        if self.forward.process(&mut a, &mut mic_spec).is_err() || self.forward.process(&mut b, &mut ref_spec).is_err() {
            return;
        }

        // Phase transform: keep only the phase of the cross spectrum.
        let hz = |hz: usize| hz * size / SAMPLE_RATE as usize;
        let (lo, hi) = (hz(ESTIMATE_HZ.0), hz(ESTIMATE_HZ.1));
        let mut cross = self.inverse.make_input_vec();
        let mean: f32 = (lo..hi).map(|k| mic_spec[k].norm() * ref_spec[k].norm()).sum::<f32>() / (hi - lo) as f32;
        if mean <= 0.0 {
            return;
        }
        for k in lo..hi {
            let g = ref_spec[k] * mic_spec[k].conj();
            cross[k] = g / (g.norm() + 1e-3 * mean);
        }
        let mut corr = self.inverse.make_output_vec();
        if self.inverse.process(&mut cross, &mut corr).is_err() {
            return;
        }

        // Index `u` of the correlation is the microphone at `start` matching
        // the system audio at `ref_start + u`, i.e. a delay of `MAX_LAG - u`.
        let range = &corr[..=MAX_LAG + LOOKAHEAD];
        let (peak, &top) = range.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1)).expect("range is not empty");
        let rest: Vec<f32> = range
            .iter()
            .enumerate()
            .filter(|(u, _)| u.abs_diff(peak) > 40)
            .map(|(_, c)| c * c)
            .collect();
        let noise = (rest.iter().sum::<f32>() / rest.len() as f32).sqrt();
        // A peak at either end is the slope of one outside the range.
        if peak == 0 || peak == range.len() - 1 || top <= 0.0 || top < MIN_CONFIDENCE * noise {
            return;
        }
        self.accept(MAX_LAG as isize - peak as isize);
    }

    /// Take a new delay: smooth small changes, and a jump once confirmed.
    fn accept(&mut self, found: isize) {
        let Some(current) = self.delay else {
            self.delay = Some(found);
            return;
        };
        if (found - current).abs() <= JUMP {
            self.delay = Some(current + (found - current) / 2);
            self.jump = None;
        } else if self.jump.is_some_and(|j| (j - found).abs() <= JUMP_AGREE) {
            self.delay = Some(found);
            self.jump = None;
            self.coupling = Coupling::default();
            self.tail.clear();
        } else {
            self.jump = Some(found);
        }
    }
}

/// FFT length of the delay estimate: the microphone window and the system
/// audio around it, without the wrap-around reaching the searched delays.
fn estimate_size() -> usize {
    (ESTIMATE_WINDOW + MAX_LAG + LOOKAHEAD).next_power_of_two()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: usize = SAMPLE_RATE as usize;

    /// Small deterministic random numbers, so the tests need no crate.
    struct Rng(u64);

    impl Rng {
        /// Uniform in `[-1, 1)`.
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        }

        fn gauss(&mut self) -> f32 {
            (0..4).map(|_| self.next()).sum::<f32>() * 0.866
        }

        fn below(&mut self, n: usize) -> usize {
            ((self.next() * 0.5 + 0.5) * n as f32) as usize % n
        }
    }

    /// Bursts of coloured noise with a syllable-like rhythm and pauses.
    fn speech(seed: u64, secs: usize, rms: f32) -> Vec<f32> {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut out = Vec::with_capacity(secs * SR);
        let mut low = 0.0;
        while out.len() < secs * SR {
            let burst = 3_200 + rng.below(9_600);
            for i in 0..burst {
                low = 0.6 * low + 0.4 * rng.gauss();
                let syllable = 0.5 + 0.5 * (std::f32::consts::TAU * 4.0 * i as f32 / SR as f32).sin().abs();
                out.push(low * syllable);
            }
            out.extend(std::iter::repeat(0.0).take(3_200 + rng.below(6_400)));
        }
        out.truncate(secs * SR);
        scale_to(&mut out, rms);
        out
    }

    fn power(x: &[f32]) -> f32 {
        x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32
    }

    fn scale_to(x: &mut [f32], rms: f32) {
        let k = rms / power(x).sqrt();
        x.iter_mut().for_each(|s| *s *= k);
    }

    /// The reference as heard by the microphone: through a sparse room
    /// response that decays over ~200 ms, delayed, times `gain`.
    fn echo_of(reference: &[f32], delay: usize, gain: f32) -> Vec<f32> {
        let mut rng = Rng(77);
        let mut taps = vec![(0usize, 1.0f32)];
        for _ in 0..100 {
            let at = 1 + rng.below(3_200);
            taps.push((at, 0.3 * rng.next() * (-(at as f32) / 464.0).exp()));
        }
        let mut out = vec![0.0; reference.len()];
        for (at, h) in taps {
            for i in (delay + at)..reference.len() {
                out[i] += gain * h * reference[i - delay - at];
            }
        }
        out
    }

    fn add_noise(x: &mut [f32], seed: u64, rms: f32) {
        let mut rng = Rng(seed);
        x.iter_mut().for_each(|s| *s += rms * rng.gauss());
    }

    fn sum(a: &[f32], b: &[f32]) -> Vec<f32> {
        a.iter().zip(b).map(|(x, y)| x + y).collect()
    }

    /// Run the canceller over both tracks, a second at a time.
    fn run(mic: &[f32], system: &[f32]) -> Vec<f32> {
        let mut ec = EchoCanceller::new();
        let mut out = Vec::new();
        for (m, s) in mic.chunks(SR).zip(system.chunks(SR)) {
            out.extend(ec.process(m, s));
        }
        out.extend(ec.flush());
        out
    }

    fn db(num: f32, den: f32) -> f32 {
        10.0 * (num.max(1e-30) / den.max(1e-30)).log10()
    }

    /// Reduction in dB from `from` seconds on.
    fn reduction(input: &[f32], output: &[f32], from: usize) -> f32 {
        db(power(&input[from * SR..]), power(&output[from * SR..]))
    }

    #[test]
    fn echo_alone_is_removed() {
        for delay_ms in [120, 300] {
            let reference = speech(1, 10, 0.1);
            let mut mic = echo_of(&reference, delay_ms * SR / 1000, 0.3);
            add_noise(&mut mic, 5, 0.0005);
            let out = run(&mic, &reference);
            assert_eq!(out.len(), mic.len());
            let r = reduction(&mic, &out, 3);
            assert!(r >= 25.0, "{delay_ms} ms: only {r:.1} dB");
        }
    }

    #[test]
    fn near_end_alone_passes_untouched() {
        let near = speech(2, 8, 0.1);
        let out = run(&near, &vec![0.0; near.len()]);
        assert_eq!(out, near);
    }

    #[test]
    fn headphones_leave_the_microphone_alone() {
        // The system audio plays, but nothing of it reaches the microphone.
        let reference = speech(1, 10, 0.1);
        let mut mic = speech(3, 10, 0.05);
        add_noise(&mut mic, 5, 0.0005);
        let out = run(&mic, &reference);
        let diff: Vec<f32> = out.iter().zip(&mic).map(|(o, m)| o - m).collect();
        let d = db(power(&diff), power(&mic));
        assert!(d < -40.0, "{d:.1} dB");
    }

    #[test]
    fn near_end_speech_over_the_echo_is_kept() {
        let reference = speech(1, 12, 0.1);
        let near = speech(4, 12, 0.15);
        let echo = echo_of(&reference, 120 * SR / 1000, 0.3);
        let mic = sum(&echo, &near);
        let out = run(&mic, &reference);
        // Frames where the near end clearly dominates.
        let (mut kept, mut total) = (0.0, 0.0);
        for f in (3 * SR / HOP)..(mic.len() / HOP) {
            let r = f * HOP..(f + 1) * HOP;
            if power(&near[r.clone()]) > 4.0 * power(&echo[r.clone()]) {
                kept += power(&out[r.clone()]);
                total += power(&near[r]);
            }
        }
        assert!(total > 0.0);
        assert!(kept >= 0.8 * total, "kept {:.0} %", 100.0 * kept / total);
    }

    #[test]
    fn clock_drift_and_a_gap_in_the_system_audio_are_survived() {
        let reference = speech(1, 16, 0.1);
        let mut mic = echo_of(&reference, 120 * SR / 1000, 0.3);
        add_noise(&mut mic, 5, 0.0005);
        // The system clock runs 0.01 % fast, and the stream drops 300 ms.
        let mut system: Vec<f32> = (0..(reference.len() as f64 / 1.0001) as usize - 2)
            .map(|i| {
                let pos = i as f64 * 1.0001;
                let (k, frac) = (pos as usize, (pos - pos.floor()) as f32);
                reference[k] + (reference[k + 1] - reference[k]) * frac
            })
            .collect();
        system.resize(mic.len(), 0.0);
        system[5 * SR..5 * SR + 3 * SR / 10].fill(0.0);
        let out = run(&mic, &system);
        let r = reduction(&mic, &out, 8);
        assert!(r >= 20.0, "only {r:.1} dB");
    }

    #[test]
    fn a_jump_of_the_delay_is_followed() {
        let reference = speech(1, 16, 0.1);
        let mut mic = echo_of(&reference, 120 * SR / 1000, 0.3);
        // From 6 s on the path is 200 ms longer (a Bluetooth speaker).
        let late = echo_of(&reference, 320 * SR / 1000, 0.3);
        mic[6 * SR..].copy_from_slice(&late[6 * SR..]);
        add_noise(&mut mic, 5, 0.0005);
        let out = run(&mic, &reference);
        let r = reduction(&mic, &out, 11);
        assert!(r >= 20.0, "only {r:.1} dB");
    }

    #[test]
    fn output_is_as_long_as_the_input_whatever_the_drains() {
        let reference = speech(1, 10, 0.1);
        let near = speech(2, 10, 0.1);
        let mut ec = EchoCanceller::new();
        let (mut out, mut m, mut s) = (Vec::new(), 0, 0);
        for (i, (&a, &b)) in [15_900, 16_100, 16_000, 7_777, 24_123, 16_000].iter().cycle().zip([16_050, 15_990, 33, 16_000].iter().cycle()).enumerate() {
            if m >= near.len() && s >= reference.len() {
                break;
            }
            let (m_end, s_end) = ((m + a).min(near.len()), (s + b).min(reference.len()));
            out.extend(ec.process(&near[m..m_end], &reference[s..s_end]));
            (m, s) = (m_end, s_end);
            assert!(i < 100);
        }
        out.extend(ec.flush());
        // No coupling, so nothing may be changed, shifted or repeated.
        assert_eq!(out, near);
    }

    #[test]
    fn reset_keeps_the_timeline() {
        let reference = speech(1, 6, 0.1);
        let mic = echo_of(&reference, 120 * SR / 1000, 0.3);
        let mut ec = EchoCanceller::new();
        let mut out = Vec::new();
        for (n, (m, s)) in mic.chunks(SR).zip(reference.chunks(SR)).enumerate() {
            if n == 3 {
                ec.reset();
            }
            out.extend(ec.process(m, s));
        }
        out.extend(ec.flush());
        assert_eq!(out.len(), mic.len());
    }

    #[test]
    fn a_new_path_is_learned_even_with_near_end_speech_at_the_switch() {
        // A Bluetooth speaker for 20 s, then laptop speakers after a reset,
        // with the user talking over the first couple of seconds.
        let secs = 32;
        let reference = speech(1, secs, 0.1);
        let mut mic = echo_of(&reference, 377 * SR / 1000, 0.3);
        let late = echo_of(&reference, 68 * SR / 1000, 0.5);
        mic[20 * SR..].copy_from_slice(&late[20 * SR..]);
        let mut near = vec![0.0; secs * SR];
        near[20 * SR..22 * SR].copy_from_slice(&speech(4, 2, 0.15));
        let echo = mic.clone();
        let mic = sum(&echo, &near);
        let mut ec = EchoCanceller::new();
        let mut out = Vec::new();
        for (n, (m, s)) in mic.chunks(SR).zip(reference.chunks(SR)).enumerate() {
            if n == 20 {
                ec.reset();
            }
            out.extend(ec.process(m, s));
        }
        out.extend(ec.flush());
        // From 3 s after the switch on there is only echo.
        let r = reduction(&mic, &out, 23);
        assert!(r >= 20.0, "only {r:.1} dB");
    }
}
