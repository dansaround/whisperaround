# WhisperAround

> superwhisper but open sourced

Dictado por voz para Windows (y macOS, [compilando desde el código](#macos-compilando-desde-el-código)): pulsa un atajo, habla y el texto aparece escrito en
la app que estés usando (correo, chat, documento…). Gratis y de código abierto.

## Descargar

**[⬇ Descargar WhisperAround para Windows](https://github.com/dansaround/whisperaround/releases/latest)**

En esa página, en **Assets**, baja el archivo que termina en **`_x64-setup.exe`**.

## Instalar

1. Abre el `…_x64-setup.exe` que descargaste.
2. Si Windows muestra **'Windows protegió tu PC'**, pulsa **Más información →
   Ejecutar de todas formas**. Aparece porque el instalador aún no está firmado
   con un certificado de pago, no porque tenga nada malo. Puedes comprobar que
   salió de este repositorio (ver [Seguridad](#seguridad-y-privacidad)).
3. Al abrirla por primera vez verás una pantalla de **Bienvenida**. Si quieres
   usar los modelos locales (sin internet), te dirá si falta Python y te dará un
   botón para instalarlo gratis desde la Microsoft Store.

La app se actualiza sola: cuando hay una versión nueva, se abre Ajustes con un
botón **Actualizar**.

## Usar

1. Elige cómo transcribir (en **Ajustes** o en el menú de la píldora):
   - **OpenAI (nube):** la más precisa. Necesita tu propia
     [API key de OpenAI](https://platform.openai.com/api-keys); el uso lo paga tu
     cuenta de OpenAI (aprox. 0,20–0,40 US$ por hora de audio).
   - **Local (faster-whisper):** gratis y el audio no sale de tu equipo. Necesita
     Python 3 y la primera vez tarda unos minutos en prepararse.
2. Pulsa **`Ctrl+Shift+X`** (configurable) para empezar a grabar. La píldora de
   arriba muestra una onda que se mueve con tu voz.
3. Pulsa de nuevo para terminar: el texto se copia y se pega solo donde tengas el
   cursor.

La píldora vive arriba en la pantalla y en la bandeja del sistema (junto al
reloj). Pasa el ratón por encima para ver sus botones; desde la bandeja puedes
abrir Ajustes o salir.

## macOS (compilando desde el código)

Aún no hay instalador publicado para Mac, pero la app funciona: vive en la barra
de menús (sin icono en el Dock) y el atajo es **`⌘⇧X`**.

1. Instala Rust (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`),
   Node 20+ y pnpm 10, y las Command Line Tools de Xcode (`xcode-select --install`).
2. Compila y copia la app:

   ```bash
   pnpm install
   pnpm build:mac
   ditto src-tauri/target/release/bundle/macos/WhisperAround.app /Applications/WhisperAround.app
   ```

3. Ábrela. La pantalla de **Bienvenida** pide los dos permisos que necesita:
   - **Micrófono**, para grabar.
   - **Accesibilidad**, para pegar solo con ⌘V. Sin él, el texto queda en el
     portapapeles.

   La app está firmada *ad hoc* (sin cuenta de Apple Developer): cada vez que la
   recompiles, macOS la ve como otra app y hay que volver a activar Accesibilidad
   (quita la entrada antigua con **−** y vuelve a añadirla).

Para los modelos locales sirve el Python de Homebrew (`brew install python`) o el
de python.org; la app los busca aunque se abra desde Finder.

## Seguridad y privacidad

Resumen para quien vaya a poner su API key (detalle técnico en
[SECURITY.md](SECURITY.md)):

- **Tu API key solo se envía a OpenAI** (`https://api.openai.com/v1/audio/transcriptions`)
  y para nada más. Está en una sola línea del código:
  [`src-tauri/src/whisper.rs`](src-tauri/src/whisper.rs), y hay un test que lo
  comprueba.
- **Se guarda cifrada en el Administrador de credenciales de Windows** (en macOS,
  en el Llavero), no en un archivo de texto. La interfaz nunca la recibe: solo
  sabe si hay una configurada.
- **Sin telemetría, sin analíticas, sin servidores propios.** Las únicas
  conexiones son: OpenAI (si usas un modelo de la nube), PyPI y Hugging Face (la
  primera vez que usas un modelo local) y GitHub (para buscar actualizaciones).
- **Los instaladores se compilan en GitHub Actions** a partir de este código,
  con una atestación de procedencia que puedes verificar.
- Consejo: crea una API key solo para WhisperAround y ponle un límite de gasto en
  OpenAI. Si algún día la quieres revocar, no afecta a nada más.

## Política de firma de código

Los instaladores se compilan exclusivamente con
[GitHub Actions](.github/workflows/release.yml) a partir de un tag de este
repositorio. Roles: autor, revisor y aprobador de releases:
[@dansaround](https://github.com/dansaround).

Privacidad: el programa no transfiere información a otros sistemas salvo lo
descrito en [Seguridad y privacidad](#seguridad-y-privacidad): la transcripción
que pides (OpenAI), la preparación de modelos locales (PyPI / Hugging Face) y la
búsqueda de actualizaciones (GitHub).

### Code signing policy

Windows releases will be signed with free code signing provided by
[SignPath.io](https://about.signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/) (application in progress; current
installers are unsigned).

- Installers are built only by [GitHub Actions](.github/workflows/release.yml)
  from a tag of this repository, with a verifiable build-provenance attestation.
- Committers and reviewers: [@dansaround](https://github.com/dansaround).
- Approvers (each signing request): [@dansaround](https://github.com/dansaround).
- Privacy: this program will not transfer any information to other networked
  systems unless specifically requested by the user, except as documented in
  [SECURITY.md](SECURITY.md) (the transcription request to OpenAI, downloading
  local models from PyPI / Hugging Face, and the update check on GitHub).

---

## Para desarrolladores

Aplicación de escritorio ultraligera (Tauri v2) para dictado instantáneo de voz a
texto usando la API de OpenAI (Whisper) o faster-whisper en local. Vive en
segundo plano, se activa con un atajo global, graba el micrófono, transcribe y
deja el texto en el portapapeles, auto-pegándolo (Ctrl/Cmd+V) en la app enfocada.

```
[Atajo global] → [cpal graba] → [WAV] → [OpenAI Whisper] → [Portapapeles + auto-paste]
      Idle      →   Recording   →                 Processing                →     Idle
```

### Arquitectura

- **Frontend** (`src/`): Svelte + Vite + TypeScript. La pill flotante, la
  ventana de ajustes (`src/lib/settings/`: Inicio, Modos, Vocabulario,
  Configuración, Sonido, Modelos e Historial) y la bienvenida, en 9 idiomas
  (`src/lib/i18n/`). Solo orquesta vía IPC; no toca la API key.
- **Backend** (`src-tauri/src/`): Rust. Todo el pipeline crítico:
  - `recorder.rs` — captura nativa del micrófono con `cpal`, encode a WAV con `hound`.
  - `whisper.rs` — POST multipart a `v1/audio/transcriptions` con `reqwest`.
  - `local.rs` — transcripción local con faster-whisper: venv privado + worker
    Python persistente (`resources/fw_server.py`) que mantiene el modelo cargado.
  - `inject.rs` — escribe el portapapeles y simula Ctrl/Cmd+V con `enigo`.
  - `pipeline.rs` — máquina de estados Idle → Recording → Processing → Idle.
  - `settings.rs` — persiste ajustes en el config dir.
  - `secret.rs` — guarda la API key en el llavero del sistema (Credential
    Manager / Keychain).
  - `shortcuts.rs` — todos los atajos (grabar, cancelar, cambiar de modo,
    push-to-talk, ratón) en un solo despachador.
  - `native_input.rs` — atajos de una sola tecla modificadora (p. ej. Ctrl
    derecho) y botones extra del ratón, con un backend por plataforma:
    `windows_input.rs` (hooks de bajo nivel de Win32) y `macos_input.rs`
    (monitores de `NSEvent`).
  - `sounds.rs`, `history.rs`, `downloads.rs`, `i18n.rs` — sonidos, historial
    local con estadísticas, descarga de modelos locales y textos del backend.
  - `win.rs` / `macos.rs` — integración de plataforma (app en primer plano para
    el historial, Mica en Windows 11, permisos de macOS).
  - `lib.rs` — setup: atajo global, system tray, ventana en background.

La API key de OpenAI nunca se envía al webview; el frontend solo recibe un flag
`apiKeyConfigured`.

### Requisitos de entorno (una sola vez)

#### 1. Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
```

#### 2. Dependencias de sistema (Linux / WSL2)

```bash
sudo apt update && sudo apt install -y \
  libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config \
  libasound2-dev libasound2-plugins
```

- `libwebkit2gtk-4.1-dev` — webview de Tauri.
- `libxdo-dev` — requerido por `enigo` (simulación de teclado, X11).
- `libasound2-dev` + `libasound2-plugins` — ALSA para `cpal`.

#### 3. Micrófono bajo WSLg (solo desarrollo)

`cpal` usa ALSA en Linux; en WSLg el mic se expone por PulseAudio. Crea
`~/.asoundrc` para enrutar ALSA → PulseAudio:

```
pcm.!default pulse
ctl.!default pulse
```

> En el build nativo de Windows esto no aplica: `cpal` usa WASAPI directamente.

### Desarrollo

```bash
pnpm install
pnpm tauri dev     # abre la ventana vía WSLg
```

### Tests

```bash
cd src-tauri && cargo test   # tests unitarios de Rust
pnpm check                   # type-check del frontend
```

CI (`.github/workflows/ci.yml`) corre ambos en cada push y PR.

### Release

1. Sube la versión en `package.json`, `src-tauri/Cargo.toml` y
   `src-tauri/tauri.conf.json`.
2. `git tag vX.Y.Z && git push origin main vX.Y.Z`.
3. `release.yml` compila en `windows-latest`, publica el release con el
   instalador, el feed del updater (`latest.json`, firmado con la clave
   `TAURI_SIGNING_PRIVATE_KEY`) y la atestación de procedencia. Si existe el
   secret `WINGET_TOKEN`, abre también el PR de la nueva versión en winget.

### Build de producción

```bash
pnpm tauri build
```

El instalador de Windows (`.exe`/MSI) se genera vía GitHub Actions en un runner
`windows-latest` (`.github/workflows/release.yml`) al pushear un tag `v*`, para
evitar el toolchain cruzado bajo WSL2.

### Notas

- Modelo configurable en Ajustes o en el dropdown de la píldora:
  - OpenAI (nube): `whisper-1`, `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`
    (por defecto).
  - Local (sin API key, el audio no sale del equipo): `faster-whisper small` y
    `faster-whisper medium` (CPU, int8). Requiere Python 3 instalado: la primera
    vez la app crea un venv en su carpeta de datos local
    (`%LOCALAPPDATA%\com.whisperaround.app\fw-venv`) e instala `faster-whisper`
    (la píldora muestra el spinner mientras tanto). Los modelos se toman de la
    caché de Hugging Face (`~/.cache/huggingface`), así que los ya descargados no
    se vuelven a bajar.
- Bajo WSLg el auto-paste actúa sobre la sesión WSLg; valida el Ctrl+V real en el
  build nativo de Windows.

### Limitaciones conocidas / TODO

- **Atajo global no siempre activo:** por ahora el atajo (`Ctrl+Shift+X`) parece
  funcionar de forma fiable solo cuando la app está abierta/enfocada; falta que
  esté "always-on" y escuche en todo momento aunque la ventana esté oculta en la
  bandeja. Pendiente: revisar el registro del `global-shortcut` y validar en build
  nativo de Windows (puede ser una limitación del grab global bajo WSLg). Falta
  además observabilidad/feedback del estado mientras está en background.
- **Cursor invisible bajo WSLg:** quirk del compositor de WSLg sobre el webview;
  no ocurre en Windows nativo (WebView2).
