# WhisperAround

> superwhisper but open sourced

Aplicación de escritorio ultraligera (Tauri v2) para dictado instantáneo de voz a
texto usando la API de OpenAI (Whisper). Vive en segundo plano, se activa con un
atajo global, graba el micrófono, transcribe vía la API y deja el texto en el
portapapeles, auto-pegándolo (Ctrl/Cmd+V) en la app enfocada.

```
[Atajo global] → [cpal graba] → [WAV] → [OpenAI Whisper] → [Portapapeles + auto-paste]
      Idle      →   Recording   →                 Processing                →     Idle
```

## Arquitectura

- **Frontend** (`src/`): Svelte + Vite + TypeScript. UI mínima — indicador de
  estado y pantalla de ajustes. Solo orquesta vía IPC; no toca la API key.
- **Backend** (`src-tauri/src/`): Rust. Todo el pipeline crítico:
  - `recorder.rs` — captura nativa del micrófono con `cpal`, encode a WAV con `hound`.
  - `whisper.rs` — POST multipart a `v1/audio/transcriptions` con `reqwest`.
  - `inject.rs` — escribe el portapapeles y simula Ctrl/Cmd+V con `enigo`.
  - `pipeline.rs` — máquina de estados Idle → Recording → Processing → Idle.
  - `settings.rs` — persiste ajustes en el config dir (la API key vive solo aquí).
  - `lib.rs` — setup: atajo global, system tray, ventana en background.

La API key de OpenAI nunca se envía al webview; el frontend solo recibe un flag
`apiKeyConfigured`.

## Requisitos de entorno (una sola vez)

### 1. Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

### 2. Dependencias de sistema (Linux / WSL2)

```bash
sudo apt update && sudo apt install -y \
  libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config \
  libasound2-dev libasound2-plugins
```

- `libwebkit2gtk-4.1-dev` — webview de Tauri.
- `libxdo-dev` — requerido por `enigo` (simulación de teclado, X11).
- `libasound2-dev` + `libasound2-plugins` — ALSA para `cpal`.

### 3. Micrófono bajo WSLg (solo desarrollo)

`cpal` usa ALSA en Linux; en WSLg el mic se expone por PulseAudio. Crea
`~/.asoundrc` para enrutar ALSA → PulseAudio:

```
pcm.!default pulse
ctl.!default pulse
```

> En el build nativo de Windows esto no aplica: `cpal` usa WASAPI directamente.

## Desarrollo

```bash
pnpm install
pnpm tauri dev     # abre la ventana vía WSLg
```

## Uso

1. Abre **Ajustes** y pega tu API key de OpenAI (se guarda en el config local).
2. Pulsa el atajo global (por defecto `Ctrl+Shift+X`) para empezar a grabar.
3. Pulsa de nuevo para detener: se transcribe y el texto se pega solo.

El indicador muestra `Listo` / `Grabando…` / `Procesando…`. Cerrar la ventana la
oculta en la bandeja; usa **Salir** desde el tray para terminar.

## Build de producción

```bash
pnpm tauri build
```

El instalador de Windows (`.exe`/MSI) se genera vía GitHub Actions en un runner
`windows-latest` (`.github/workflows/release.yml`) al pushear un tag `v*`, para
evitar el toolchain cruzado bajo WSL2.

## Notas

- Modelo configurable en Ajustes: `whisper-1` (por defecto), `gpt-4o-transcribe`,
  `gpt-4o-mini-transcribe`.
- Bajo WSLg el auto-paste actúa sobre la sesión WSLg; valida el Ctrl+V real en el
  build nativo de Windows.

## Limitaciones conocidas / TODO

- **Atajo global no siempre activo:** por ahora el atajo (`Ctrl+Shift+X`) parece
  funcionar de forma fiable solo cuando la app está abierta/enfocada; falta que
  esté "always-on" y escuche en todo momento aunque la ventana esté oculta en la
  bandeja. Pendiente: revisar el registro del `global-shortcut` y validar en build
  nativo de Windows (puede ser una limitación del grab global bajo WSLg). Falta
  además observabilidad/feedback del estado mientras está en background.
- **Cursor invisible bajo WSLg:** quirk del compositor de WSLg sobre el webview;
  no ocurre en Windows nativo (WebView2).
