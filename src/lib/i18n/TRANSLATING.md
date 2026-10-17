# Translating WhisperAround

WhisperAround is a menu-bar / tray dictation app for macOS and Windows. You
press a shortcut, speak, and the text is transcribed and pasted into whatever
app has focus. The UI should read like a native system app: short, calm, and
precise. A few strings have a Windows variant (keys ending in `Windows`);
those use Microsoft's terms instead of Apple's.

English is the source language. Supported locales:

| Code | Language                       | `Intl` tag | macOS `.lproj` |
| ---- | ------------------------------ | ---------- | -------------- |
| `en` | English                        | `en`       | `en`           |
| `es` | Spanish (neutral / Latin Am.)  | `es`       | `es`           |
| `pt` | **Brazilian** Portuguese       | `pt-BR`    | `pt_BR`        |
| `fr` | French                         | `fr`       | `fr`           |
| `de` | German                         | `de`       | `de`           |
| `it` | Italian                        | `it`       | `it`           |
| `ko` | Korean                         | `ko`       | `ko`           |
| `ja` | Japanese                       | `ja`       | `ja`           |
| `zh` | **Simplified** Chinese         | `zh-Hans`  | `zh-Hans`      |

## 1. Where the strings live

| What | English source | Translation file |
| --- | --- | --- |
| Settings window, pill, welcome window | `src/lib/i18n/locales/en/<namespace>.ts` | `src/lib/i18n/locales/<code>.ts` |
| Tray menu, window titles, error messages from the backend | `src-tauri/i18n/en.json` | `src-tauri/i18n/<code>.json` |
| macOS microphone permission prompt | `src-tauri/lproj/en.lproj/InfoPlist.strings` | `src-tauri/lproj/<lproj>/InfoPlist.strings` (already translated) |

**Frontend files.** Each `<code>.ts` is a `PartialDict`, which is a nested object with
the same shape as the English one (`common`, `pill`, `welcome`, `shell`, `home`,
`modes`, `vocabulary`, `configuration`, `sound`, `models`, `history`). Missing
keys fall back to English, so a partial file still works. Only add the keys
that also exist in English. The type checker rejects unknown keys.

```ts
import type { PartialDict } from "../index.svelte";

const dict: PartialDict = {
  pill: { status: { idle: "Listo para dictar" } },
};
export default dict;
```

**Backend files.** These are flat JSON `"key": "text"` maps. Keep the key set
identical to `en.json`. The Rust unit tests check that every `{placeholder}`
is still there after translation.

## 2. Tone and style

- **Concise macOS system-UI voice.** Write the way System Settings, Finder or
  Notes would say it in your language. Avoid marketing language, exclamation
  marks and filler.
- **Address the user** the way Apple does in your language:
  - es: *tú*
  - pt-BR: *você*
  - fr: *vous*
  - de: *du* (Apple's current German UI uses *du*; never use *Sie*)
  - it: *tu*
  - ja: neutral polite (*です/ます* in sentences; plain noun phrases for labels and buttons)
  - ko: *합니다/하세요* style in sentences; noun phrases for labels
  - zh: neutral, no *您* needed
- **Capitalization follows your language's Apple conventions.** English uses
  Title Case in a few places (`Toggle Recording`, `Push to Talk`, `Check Again`,
  `Open System Settings`). Don't copy that into languages that use sentence
  case, such as es, pt, fr, it and de (de only capitalizes nouns).
- **Buttons are verbs** (`Download`, `Use`, `Save`, `Continue`). Use the
  infinitive or imperative form your language uses on Apple buttons.
- **Ellipsis**: keep `…` (one character, U+2026) where English has it. It
  marks a state in progress (`Saving…`) or a menu item that opens a window
  (`Settings…`).
- **Quotes**: English uses curly quotes “…” to quote UI labels. Use your
  language's quotation marks: « … » (fr, with narrow no-break spaces), „…“ (de),
  « … » or “…” (it, es, pt), 「…」 (ja), ‘…’ or “…” (ko), “…” (zh).
- **Path separator** `›` joins menu paths (`Settings › Models library`). Keep it.
- **Space limits.** Several strings sit in small fixed-width slots. See §7.

## 3. Glossary

Keep each term the same everywhere in the app (frontend and backend).

| English | Meaning / guidance |
| --- | --- |
| **pill** | The small floating capsule at the top or bottom of the screen that shows recording status. It's a product term, like Apple's "Dynamic Island". Translate it as a short, friendly noun for a capsule or pill shape (for example es *píldora*, fr *pastille*, de *Pille* or *Kapsel*, ja *ピル*, zh *胶囊*). Pick one term and use it everywhere. |
| **mode** | A saved preset: a name, an icon, a voice model and a language. The user switches between modes with a shortcut. Use the ordinary word for "mode" (*modo*, *mode*, *Modus*, *modalità*, *モード*, *모드*, *模式*). |
| **vocabulary** | The user's list of custom words (names, brands, jargon) that are passed to the speech model as spelling hints. Use the ordinary word for "vocabulary". A **term** is one entry in that list. |
| **shortcut** | A keyboard shortcut (or a mouse-button shortcut). Use Apple's term for your language: es *atajo de teclado*, pt-BR *atalho*, fr *raccourci*, de *Tastaturkurzbefehl* or *Kurzbefehl*, it *abbreviazione* or *scorciatoia*, ja *ショートカット*, ko *단축키*, zh *快捷键*. |
| **Toggle Recording** | The action that starts recording, or stops it if it's already running. "Toggle" means start/stop with one key. Translate the meaning ("Start/stop recording"), not the word "toggle". The same text appears as a shortcut name, a tray menu item and in errors (see §6). |
| **Push to Talk** | Hold a key to record, release it to stop. Use the common term in your language if there is one (fr *Appuyer pour parler*, de *Push-to-Talk* is acceptable, ja *押して話す*, zh *按住说话*). |
| **record / recording** | Capturing audio from the microphone. It's not video recording. |
| **dictate / dictation** | Speaking text for the app to type. Use the word Apple uses for macOS Dictation in your language (*dictar*, *ditar*, *dicter*, *diktieren*, *dettare*, *音声入力* or *口述*, *받아쓰기*, *听写*). |
| **transcript** | The text produced from one recording. It's a noun. Entries in History are transcripts. |
| **transcription** / **transcribe** | The process of turning audio into text. `Transcribing…` is the status shown while waiting. |
| **voice model** / **model** | A speech-recognition model such as `gpt-4o-transcribe` or `faster-whisper small`. Translate "model" and keep the model ids. |
| **local** (model) | Runs on this Mac, offline. Translate it as "local" or "on-device". It's a heading and a badge, so keep it to one short word. |
| **cloud** (model) | Runs on OpenAI's servers over the internet. |
| **API key** | An OpenAI secret key. Keep the term "API key" in its common localized form (es *clave de API*, pt-BR *chave de API*, fr *clé API*, de *API-Schlüssel*, it *chiave API*, ja *APIキー*, ko *API 키*, zh *API 密钥*). |
| **keychain** | The macOS Keychain. Use Apple's name: es *llavero*, pt-BR *Chaves*, fr *trousseau*, de *Schlüsselbund*, it *portachiavi*, ja *キーチェーン*, ko *키체인*, zh *钥匙串*. |
| **Credential Manager** (`*Windows` keys) | Where Windows keeps the API key. Use Microsoft's name: es *Administrador de credenciales*, pt-BR *Gerenciador de Credenciais*, fr *Gestionnaire d'identification*, de *Anmeldeinformationsverwaltung*, it *Gestione credenziali*, ja *資格情報マネージャー*, ko *자격 증명 관리자*, zh *凭据管理器*. |
| **clipboard** | Use the system term: *portapapeles*, *área de transferência*, *presse-papiers*, *Zwischenablage*, *appunti*, *クリップボード*, *클립보드*, *剪贴板*. |
| **paste automatically / auto-paste** | After transcribing, the app sends ⌘V to the focused app. |
| **Accessibility** (permission) | The macOS privacy permission that lets apps control the keyboard. Always use Apple's name for that pane (see §5). |
| **History** | The local list of past transcripts. |
| **theme** | Light / Dark / Auto appearance. Use Apple's appearance terms for your language (fr *Clair / Sombre / Auto*, de *Hell / Dunkel / Automatisch*, and so on). |
| **Pill look** | The pill's visual style ("skin"): *Standard* or *Onyx*. |
| **Recording window** | How the pill looks while recording: *Classic* (a large panel), *Mini* (a small capsule) or *None* (hidden). |
| **WPM** | Words per minute. Use the usual local abbreviation if there is one (fr *mots/min*, de *WpM*, ja *WPM* or *語/分*, zh *字/分钟*). Otherwise keep *WPM*. |

## 4. Never translate

- The brand **WhisperAround**, and **OpenAI**, **Whisper**, **faster-whisper**,
  **mlx-whisper**, **MLX**, **Python**, **python.org**, **Microsoft Store**,
  **Apple** and **Homebrew**.
- Model ids and labels: `gpt-4o-mini-transcribe`, `gpt-4o-transcribe`,
  `whisper-1`, `faster-whisper small/medium`, `whisper large-v3 turbo`.
- Sound names **Orus**, **Loto**, **Okaso**, and the pill skin **Onyx**. (Their
  descriptions *are* translated.)
- Keyboard glyphs and key names: `⌘ ⌥ ⇧ ⌃ ⌫ esc Esc ⌘V Ctrl+V ⌥⇧K`.
- Shell commands and code: `brew install python`, `pip install …`,
  `python -m venv`.
- `api.openai.com`, `sk-…`, and anything inside `{braces}`.
- Language names in the language pickers. They are always shown in their own
  language and live in code, not in the dictionaries.

## 5. macOS names (use Apple's own localization)

Several strings point to System Settings panes. Use the exact names macOS
shows in your language (macOS 13+). Check them on a Mac if you can.

| Locale | System Settings › Privacy & Security › Accessibility / Microphone |
| --- | --- |
| es | Ajustes del Sistema › Privacidad y seguridad › Accesibilidad / Micrófono |
| pt-BR | Ajustes do Sistema › Privacidade e Segurança › Acessibilidade / Microfone |
| fr | Réglages Système › Confidentialité et sécurité › Accessibilité / Micro |
| de | Systemeinstellungen › Datenschutz & Sicherheit › Bedienungshilfen / Mikrofon |
| it | Impostazioni di Sistema › Privacy e sicurezza › Accessibilità / Microfono |
| ja | システム設定 › プライバシーとセキュリティ › アクセシビリティ / マイク |
| ko | 시스템 설정 › 개인정보 보호 및 보안 › 손쉬운 사용 / 마이크 |
| zh | 系统设置 › 隐私与安全性 › 辅助功能 / 麦克风 |

These names appear in:

- `welcome.microphone.openSystemSettings`
- `welcome.accessibility.bodyAfter`
- backend `error.mic.silent`, `error.mic.blocked` and `error.paste.accessibility`

## 6. Placeholders, plurals and split sentences

### Placeholders

`{name}` tokens are replaced at runtime. In every translation:

- Keep each token **exactly** as written: same spelling, ASCII braces, and do
  not translate the word inside the braces.
- Move tokens wherever your grammar needs them.
- Don't add tokens that aren't in the English string.

| Key | Token | Value |
| --- | --- | --- |
| `shell.mic.defaultDevice` | `{name}` | Microphone name, e.g. "MacBook Pro Microphone (Default)". |
| `shell.update.install` | `{version}` | Version number, e.g. "0.1.7". |
| `shell.toast.loadFailed` | `{error}` | An error message, already translated. |
| `home.shareText` | `{range}` `{wpm}` `{words}` `{saved}` | `{range}` is a `home.range.*` label. `{words}` is `home.shareWords` (e.g. "1,204 words"). `{saved}` is a `home.time.*` value, and the sentence ends with "… saved". |
| `home.stats.savedTooltip` | `{wpm}` | The number 40. |
| `home.changes.modesVocabulary.body` | `{keys}` | The glyphs `⌥⇧K`. |
| `modes.tip.body` | `{keys}` | **Special.** The code splits the string at `{keys}` and inserts keycap graphics (or `modes.tip.noShortcut`). `{keys}` must appear exactly once. |
| `configuration.shortcuts.tip` | `{cmd}` `{delete}` | **Special.** Replaced by bold ⌘ and ⌫ glyphs. Each must appear exactly once. "Right {cmd}" means the right-hand Command key. |
| `configuration.general.autoPasteSub` | `{keys}` | "⌘V" (or "Ctrl+V"). |
| `configuration.mouseButtons.other` | `{n}` | A mouse-button number. |
| `sound.preview.play` | `{name}` | A `sound.preview.events.*` title, e.g. "Play Start recording". This is a screen-reader label. |
| `models.card.progress` | `{received}` `{total}` | Formatted sizes, e.g. "312 MB of 1.6 GB". |
| `models.card.ready` | `{model}` | A model label (untranslated). |
| `vocabulary.remove` | `{word}` | One of the user's vocabulary terms. Screen-reader label. |
| `history.seconds` | `{s}` | Seconds, e.g. "12s". Use your local short unit. |
| `history.minutesSeconds` | `{m}` `{s}` | Minutes and seconds, e.g. "1m 05s". |
| `history.empty.noMatchesSub` | `{query}` | The user's search text. |

Backend (`en.json`) tokens that carry **raw values** must not be translated
or reworded around in a way that hides them:

- `{error}` is an inner error message, often from the OS and often in
  English.
- `{command}` is a literal command, e.g. `pip install faster-whisper`.
- `{status}` is an HTTP status code.
- `{body}` is an OpenAI response body.
- `{output}` is stderr text.
- `{field}` and `{value}` are a setting id (e.g. `pillStyle`) and its value.
- `{file}`, `{path}`, `{model}`, `{sound}`, `{kind}`, `{key}`, `{button}`,
  `{shortcut}`, `{received}`, `{size}` and `{format}` are raw values.
- `{name}` and `{other}` in `shortcut.error.*` are `shortcut.name.*` strings.
- `{hint}` in `error.python.missing` is a whole sentence
  (`error.python.hintMac`).

### Plurals (frontend only)

Plural keys are objects such as `{ one: "{count} word", other: "{count} words" }`.
`{count}` is already formatted for the locale (e.g. "1.204" in German).

- The categories come from `Intl.PluralRules` for your locale. Supported:
  `zero`, `one`, `two`, `few`, `many`, `other`.
- `other` is **required**. If your locale doesn't use a category, leave it
  out. The code falls back to `other`.
- **ja, ko, zh** only need `other`.
- **fr** and **pt-BR** use `one` for 0 and 1.
- **es** and **it** may also use `many` for very large round numbers, but
  `other` is enough.
- Plural keys:
  - `home.shareWords`
  - `home.time.minutes`, `home.time.hours`, `home.time.days`
  - `vocabulary.count`
  - `history.words`

### Sentences split around markup

Some sentences are split so that a bold word or a code snippet can sit in the
middle. Translate the pieces so they read as **one sentence** when joined. You
may move text between the pieces, and a piece may be empty (`""`).

- `welcome.accessibility.bodyBefore` + **WhisperAround** + `welcome.accessibility.bodyAfter`
- `welcome.python.neededBefore` + **`neededBold`** + `welcome.python.neededAfter`.
  In English, `neededBefore` is empty and the bold word is "Local".
- `welcome.python.installMacBefore` + `brew install python` + `welcome.python.installMacAfter`.
  `installMacAfter` begins right after the code with no space. In English it
  starts with ")".

The pieces are joined with a single space, except before `installMacAfter`.

## 7. Space-limited strings

- `models.card.speed` and `models.card.accuracy` sit in a 60 px label. Use one
  short word.
- `shell.nav.*`: sidebar items, about 140 px.
- `pill.tooltip.*`, `pill.hint.*`, `pill.status.*`: small pill text. Keep them
  very short.
- `home.stats.*`, `home.savedIn.*`: four columns. Keep them short.
- Badges: `models.openai.configured` and `models.openai.missing`,
  `models.card.offline`, `models.card.cloud`, `models.card.active`,
  `modes.active`, `sound.start.default`, `welcome.state.*`. Use one or two
  words.
- `configuration.appearance.themes.*`, `.styles.*`, `.positions.*`,
  `.skins.*`: captions under small thumbnails.

## 8. Strings that must match each other

| These keys | Must use the same wording as |
| --- | --- |
| `configuration.shortcuts.toggle` / `.cancel` / `.mode` / `.ptt` / `.mouse` (frontend) | backend `shortcut.name.toggle` / `.cancel` / `.changeMode` / `.pushToTalk` / `.mouse`. They are shown in duplicate-shortcut errors. |
| `configuration.shortcuts.toggle` | backend `tray.toggle` |
| `modes.tip.noShortcut` (it quotes the shortcut's name) | `configuration.shortcuts.mode` |
| `history.empty.offSub` (it quotes the toggle) | `configuration.general.saveHistory` = `history.saveHistory` |
| `welcome.python.afterInstall` (it quotes the button) | `welcome.python.checkAgain` |
| backend `error.python.hintMac` / `hintWindows` ("tray › Welcome…") | `tray.welcome` |
| backend `error.local.notDownloaded` ("Settings › Models library") | `shell.nav.models` |
| `welcome.cloudNote`, backend `error.openai.noKey` ("Settings") | the Settings window (`pill.aria.settings`) |
| `welcome.title`, backend `window.welcomeTitle` | each other |
| `models.groups.local`, `modes.localGroup`, `pill.menu.local`, `welcome.python.neededBold` | each other ("Local" = on-device) |

## 9. Per-key context for ambiguous strings

### pill

- `pill.status.idle` "Ready to dictate": screen-reader label and expanded-panel text.
- `pill.status.error` "Something went wrong": generic error, shown only when the backend sent no message.
- `pill.unknownError` "Unknown error": shown when the error event carries no message.
- `pill.tooltip.cancel` and `pill.hint.cancel` "Cancel": cancel (discard) the current recording.
- `pill.hint.close` "Close": collapse the expanded panel. It's followed by an `Esc` key chip.
- `pill.hint.record` / `pill.hint.stop` are followed by the shortcut's key chips. They are verbs.
- `pill.aria.collapseEsc` "Collapse (Esc)": a native tooltip. Keep "(Esc)".
- `pill.tooltip.model` "Model": the button that opens the model picker.

### welcome

- `welcome.state.granted` "Allowed", `denied` "Blocked", `notRequested` "Not requested": permission status badges.
- `welcome.state.missing` "Missing": Accessibility permission isn't granted.
- `welcome.state.notFound` "Not found": Python wasn't found.
- `welcome.microphone.body` "macOS will ask you once.": the system permission prompt.
- `welcome.language` "Language": tooltip of the UI-language picker. It's not the transcription language.

### home

- `home.range.all` "All time", `week` "Last 7 days", `today` "Today": statistics period picker.
- `home.savedIn.*` "Saved all time" …: the label under the *time saved* number. It means "time saved (by not typing) during this period".
- `home.time.*` "{count} min / h / day(s)": short duration units for time saved.
- `home.stats.appsUsed` "Apps used": number of different apps you dictated into.
- `home.copyStats` "Copy stats": tooltip of the share button. It copies a summary to the clipboard.
- `home.changes.*`: "What's new?" release notes. Translate them freely, as marketing-light copy.

### modes

- `modes.newModeName` "New mode": the default name of a newly created mode. It becomes user data once saved.
- `modes.active` "Active": a badge. `modes.activeMode` "Active mode" is a tooltip on the dot next to the active mode.
- `modes.useThisMode` "Use this mode": a button that makes the mode active.
- `modes.icons.*`: screen-reader names of the icon choices (Sparkle, Message, Laptop, Pen, Code, Globe, Bolt = lightning bolt).
- `modes.tip.dismiss` "Dismiss": hides the tip permanently.

### vocabulary

- `vocabulary.empty.sub` "… Try:": followed by clickable example words.
- `vocabulary.placeholder`: input hint. Users can paste a comma- or newline-separated list.

### configuration

- `configuration.appearance.alwaysShow` "Always show": keep the pill visible when idle.
- `configuration.appearance.styles.none` "None": no recording window at all. Use the feminine or masculine form that agrees with your word for "window", or a neutral "None".
- `configuration.appearance.positions.top` / `bottom`: where the pill sits on screen.
- `configuration.appearance.skins.standard` "Standard": the default pill look, shown next to "Onyx".
- `configuration.shortcuts.toggleSub` "Starts and stops recordings": third-person descriptions. Use whatever form your language uses for setting descriptions.
- `configuration.shortcuts.mouseSub` "Tap to toggle, or hold and release when done": tap = quick click, so the shortcut works either as start/stop or as push-to-talk.
- `configuration.recorder.needsAccessibility` "Needs Accessibility": a small warning button. Single-key shortcuts need the Accessibility permission.
- `configuration.recorder.resetTitle` "Reset to default" and `resetLabel` "Reset shortcut": tooltip and screen-reader label of the same button.
- `configuration.recorder.placeholder` "Record shortcut": an empty shortcut field. "Record" means *capture a key combination*, not audio. Choose a verb that doesn't suggest audio recording if your language distinguishes them (e.g. de *Kurzbefehl festlegen*).
- `configuration.recorder.pressKeys` / `pressButton`: shown while waiting for input.
- `configuration.mouseButtons.back` / `forward`: the side buttons on a mouse (browser back/forward).
- `configuration.general.languageSub` "Used for menus, the pill and messages.": the UI language.

### sound

- `sound.start.default` "Default": badge on the factory-default start sound.
- `sound.effects.mute` / `max`: tooltips on the volume icons (volume 0% / 100%).
- `sound.preview.events.*`: names of the moments that play a sound.

### models

- `models.openai.configured` / `missing`: badges for the API key state.
- `models.openai.replacePlaceholder` "••••••••••••  (replace)": a password-field placeholder when a key is already saved. Keep the dots and translate only "(replace)".
- `models.activeNote` "Picking a model sets it for your active mode": followed by a badge with the mode's name.
- `models.card.use` "Use": makes this model active. `models.card.active` "Active" is a badge.
- `models.card.offline` "Offline": works without internet. `models.card.cloud` "Cloud": needs internet.
- `models.card.preparingEngine` "Preparing engine…": installing Python packages before the download.
- `models.python.getPython` "Get Python": opens python.org.

### history

- `history.clear` "Clear": opens the confirmation. `history.deleteAll` "Delete all" confirms. `history.cancel` "Cancel" backs out.
- `history.clearSearch` "Clear search": screen-reader label of the ✕ in the search field.
- `history.copy` / `history.delete`: tooltips for one entry.
- `history.today` / `yesterday`: day headings. Other days use the localized date format automatically.

### backend (`src-tauri/i18n/en.json`)

- **Lowercase fragments.** These keys are always shown after a prefix such as
  "Microphone error: " or "{name}: ", so they start lowercase and have no
  final period. Keep them lowercase where your language allows.
  - `error.mic.noDevice`, `config`, `sampleFormat`, `buildStream`, `startStream`
  - `error.recording.threadEnded`, `threadPanicked`
  - `error.paste.mainThread`, `notRun`, `keyboard`, `modifier`, `keyV`, `release`
  - `error.local.workerExited`
  - `shortcut.error.unknownModifier`, `invalidMouseButton`, `invalid`
- `error.paste.keyV` "V key: {error}": the V of ⌘V.
- `error.python.notFound` "Python not found": short, inserted into `error.python.missing`.
- `settings.invalidValue` "Invalid value for {field}: '{value}'": mostly seen by developers. Keep the quotes around `{value}`.
- `tray.show` "Show pill": a tray menu item that briefly reveals the pill.
- `tray.settings` "Settings…" and `tray.welcome` "Welcome…": menu items that open a window, so they keep the `…`.

## 10. Formatting is automatic

Dates, times, numbers, percentages and byte sizes are formatted with `Intl`
for the active locale (`intlLocale()`). Don't put numbers or date formats in
translations. Units that are part of a string (`{s}s`, `{count} min`, `WPM`)
are yours to localize.
