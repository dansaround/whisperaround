# -*- coding: utf-8 -*-
"""Local faster-whisper worker for WhisperAround.

Spawned once by the Rust backend (`src-tauri/src/local.rs`) and kept alive so
models stay loaded between dictations. Speaks JSON lines over stdin/stdout:

    -> {"cmd": "load", "model": "small"}
    -> {"cmd": "transcribe", "model": "medium", "path": "C:/.../audio.wav"}
    -> {"cmd": "load", "model": "turbo", "mlx": "/.../whisper-large-v3-turbo"}
    <- {"ok": true, "text": "..."} | {"ok": false, "error": "..."}

Models are fetched from (or reused in) the Hugging Face cache, so the
`Systran/faster-whisper-*` checkpoints already downloaded are not fetched again.
Requests with an `mlx` folder run that (already downloaded) checkpoint with
mlx-whisper on the Apple Silicon GPU instead.
"""
import json
import sys
import wave

# Libraries may print to stdout; keep that channel for the protocol only.
PROTO = sys.stdout
sys.stdout = sys.stderr

DEVICE, COMPUTE = "cpu", "int8"
SR = 16000  # what Whisper expects
_models = {}


def reply(obj):
    PROTO.write(json.dumps(obj) + "\n")
    PROTO.flush()


def get_model(name):
    if name not in _models:
        from faster_whisper import WhisperModel

        _models[name] = WhisperModel(name, device=DEVICE, compute_type=COMPUTE)
    return _models[name]


def load_wav(path):
    """16-bit mono WAV -> float32 @ 16 kHz, without PyAV (whose newer releases
    break faster-whisper's own decoder)."""
    import numpy as np

    with wave.open(path, "rb") as w:
        rate, channels = w.getframerate(), w.getnchannels()
        pcm = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16)
    audio = pcm.astype(np.float32) / 32768.0
    if channels > 1:
        audio = audio.reshape(-1, channels).mean(axis=1)
    if rate != SR and audio.size:
        n = int(round(audio.size * SR / rate))
        audio = np.interp(
            np.linspace(0, audio.size - 1, n), np.arange(audio.size), audio
        ).astype(np.float32)
    return audio


def speech_only(audio):
    """Drop the silences with faster-whisper's VAD (mlx-whisper has none and
    makes up text over silence). Empty when nobody spoke."""
    import numpy as np
    from faster_whisper.vad import VadOptions, get_speech_timestamps

    spans = get_speech_timestamps(audio, VadOptions(min_silence_duration_ms=500))
    if not spans:
        return audio[:0]
    return np.concatenate([audio[s["start"]:s["end"]] for s in spans])


def transcribe_mlx(folder, audio, language=None, prompt=None):
    import mlx_whisper

    result = mlx_whisper.transcribe(
        audio,
        path_or_hf_repo=folder,
        verbose=None,  # no progress bar
        condition_on_previous_text=False,
        language=language or None,
        initial_prompt=prompt or None,
    )
    return " ".join(s["text"].strip() for s in result["segments"] if s["text"].strip())


def warm_mlx(folder):
    """Load the weights and compile the GPU kernels on a second of silence."""
    import numpy as np

    transcribe_mlx(folder, np.zeros(SR, dtype=np.float32))


def transcribe(name, path, language=None, prompt=None, mlx=None):
    if mlx:
        audio = speech_only(load_wav(path))
        return transcribe_mlx(mlx, audio, language, prompt) if audio.size else ""
    segments, _info = get_model(name).transcribe(
        load_wav(path),
        beam_size=5,
        vad_filter=True,
        condition_on_previous_text=False,
        language=language or None,
        initial_prompt=prompt or None,
    )
    return " ".join(s.text.strip() for s in segments if s.text.strip())


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            cmd = req.get("cmd")
            if cmd == "load" and req.get("mlx"):
                warm_mlx(req["mlx"])
                reply({"ok": True, "text": ""})
            elif cmd == "load":
                get_model(req["model"])
                reply({"ok": True, "text": ""})
            elif cmd == "transcribe":
                reply({
                    "ok": True,
                    "text": transcribe(
                        req["model"],
                        req["path"],
                        req.get("language"),
                        req.get("prompt"),
                        req.get("mlx"),
                    ),
                })
            else:
                reply({"ok": False, "error": "comando desconocido: %s" % cmd})
        except Exception as e:  # report every failure back instead of dying
            reply({"ok": False, "error": "%s: %s" % (type(e).__name__, e)})


if __name__ == "__main__":
    main()
