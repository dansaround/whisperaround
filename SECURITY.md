# Seguridad

Este documento es para quien quiera auditar qué hace WhisperAround con tu API
key de OpenAI y con tu audio antes de usarla. Todo lo que se afirma aquí apunta
al código que lo implementa.

## La API key de OpenAI

| Pregunta | Respuesta | Dónde verlo |
| --- | --- | --- |
| ¿A dónde se envía? | Solo a `https://api.openai.com/v1/audio/transcriptions`, como cabecera `Authorization: Bearer`, cuando transcribes con un modelo de OpenAI. | [`src-tauri/src/whisper.rs`](src-tauri/src/whisper.rs) (único uso de la key; test `key_goes_only_to_openai_over_https`) |
| ¿Dónde se guarda? | Windows: Administrador de credenciales (cifrado con DPAPI, ligado a tu usuario). macOS: Llavero. Linux: archivo de ajustes de la app. Nunca en `settings.json` en Windows/macOS. | [`src-tauri/src/secret.rs`](src-tauri/src/secret.rs), [`src-tauri/src/settings.rs`](src-tauri/src/settings.rs) (test `key_stays_out_of_the_file_when_the_os_store_exists`) |
| ¿La ve la interfaz (webview)? | No. El frontend solo recibe `apiKeyConfigured: true/false`; la key nunca vuelve del proceso Rust. | `SettingsView` en [`settings.rs`](src-tauri/src/settings.rs) (test `view_never_contains_the_key`) |
| ¿Se registra en logs? | No. Los errores de OpenAI se muestran tal cual los devuelve la API (que enmascara la key). | [`whisper.rs`](src-tauri/src/whisper.rs) |

Las versiones anteriores a la 0.1.5 guardaban la key en `settings.json`; al
abrir la 0.1.5 se mueve automáticamente al llavero y se borra del archivo.

**Recomendación:** crea en OpenAI una API key exclusiva para WhisperAround, con
límite de gasto. Si la revocas, no afecta a nada más.

## Conexiones de red

La app no tiene telemetría, analíticas ni servidores propios. Estas son todas
las conexiones que hace:

| Destino | Cuándo | Qué se envía |
| --- | --- | --- |
| `api.openai.com` | Al transcribir con un modelo de OpenAI | El audio grabado (WAV), tu API key y, si los configuras, el idioma y tu vocabulario |
| `pypi.org` | La primera vez que usas un modelo local | Descarga del paquete `faster-whisper` en un entorno privado de la app |
| `pypi.org` | Al elegir 'GPU' en Modelos › Procesador | Descarga de `nvidia-cublas-cu12` y `nvidia-cudnn-cu12` (bibliotecas CUDA de NVIDIA) en el mismo entorno privado |
| `huggingface.co` | La primera vez que usas cada modelo local (o al descargar uno desde Modelos, en Mac) | Descarga del modelo, fijada a una revisión concreta |
| `github.com` | Al abrir la app | Consulta si hay versión nueva (`latest.json`); no envía datos tuyos |

Con un modelo local, el audio no sale de tu equipo.

Además, la interfaz corre con una Content Security Policy estricta
([`tauri.conf.json`](src-tauri/tauri.conf.json)): no puede cargar scripts ni
conectarse a ningún sitio externo, solo hablar con el proceso Rust de la app.

## Permisos que usa y por qué

- **Micrófono:** grabar mientras dictas (solo entre pulsar y volver a pulsar el
  atajo) o mientras grabas una reunión.
- **Audio del sistema (Windows):** si eliges 'Audio del sistema' o 'Micrófono +
  audio del sistema', la app abre una captura *loopback* (WASAPI) de la salida de
  audio predeterminada, solo mientras graba. No cambia ni silencia nada.
  Código: [`src-tauri/src/recorder.rs`](src-tauri/src/recorder.rs).
- **Modo reunión:** solo empieza cuando tú lo inicias (Ajustes, bandeja o
  atajo) y la píldora lo indica mientras dura. La primera vez pide confirmar un
  aviso de consentimiento. Código: [`src-tauri/src/meeting.rs`](src-tauri/src/meeting.rs).
- **Portapapeles + simular `Ctrl+V`:** pegar el texto transcrito donde está el
  cursor (se puede desactivar en Ajustes).
- **Atajo global de teclado:** empezar/terminar el dictado desde cualquier app.
  Los atajos normales (p. ej. `Ctrl+Shift+X`) se registran en el sistema y la
  app solo se entera de esa combinación.
- **Hook de teclado y ratón de bajo nivel (Windows):** solo para los atajos de
  una sola tecla (p. ej. Ctrl derecho) o de botones extra del ratón, y solo se
  instala si configuras uno (o mientras grabas uno en Ajustes). Windows le pasa
  todas las pulsaciones, pero de las teclas normales la app solo anota que hubo
  *una* pulsación, sin guardar cuál; solo actúa sobre Ctrl, Alt, Shift y Win y
  los botones central/laterales del ratón. Nunca bloquea teclas, no las guarda y
  no las envía a ningún sitio.
  Código: [`src-tauri/src/windows_input.rs`](src-tauri/src/windows_input.rs).
- **App en primer plano (historial):** al grabar, anota el nombre de la app
  activa (p. ej. 'Google Chrome') para las estadísticas.

## Datos que se guardan en tu equipo

- **Ajustes:** `settings.json` en la carpeta de configuración de la app (sin la
  API key en Windows/macOS).
- **Historial:** `history.json` en la carpeta de datos de la app, con el texto
  de hasta 1000 transcripciones, su duración, modelo, modo y la app donde
  dictaste, más estadísticas totales. Está activado por defecto; se desactiva o
  se borra en Ajustes › Historial. Nunca sale de tu equipo.
- **Reuniones:** la transcripción de cada reunión va al mismo historial (si está
  activado). Al exportarla se escribe un `.md` en `Documentos/WhisperAround`.
  El audio de las reuniones no se guarda: cada tramo se borra tras transcribirlo.

## Verificar que un instalador viene de este código

Los instaladores se compilan en GitHub Actions
([`release.yml`](.github/workflows/release.yml)) a partir del tag publicado,
y cada uno lleva una atestación de procedencia firmada por GitHub:

```bash
gh attestation verify WhisperAround_0.1.7_x64-setup.exe -R dansaround/whisperaround
```

Las actualizaciones automáticas solo se instalan si su firma coincide con la
clave pública incluida en la app.

## Reportar una vulnerabilidad

Abre un [aviso de seguridad privado](https://github.com/dansaround/whisperaround/security/advisories/new)
en GitHub (no un issue público).
