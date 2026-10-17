/** English strings: welcome. Source of truth for every other locale. */
export default {
  title: "Welcome to WhisperAround",
  subtitle: "Dictate in any app: press the shortcut, speak, and the text pastes itself.",
  /** Accessible label of the language picker in the header. */
  language: "Language",
  /** Permission / check states shown at the right of each card. */
  state: {
    granted: "Allowed",
    denied: "Blocked",
    notRequested: "Not requested",
    missing: "Missing",
    checking: "Checking…",
    notFound: "Not found",
  },
  microphone: {
    title: "Microphone",
    body: "To record your voice. macOS will ask you once.",
    allow: "Allow Microphone",
    openSystemSettings: "Open System Settings",
  },
  accessibility: {
    title: "Accessibility",
    /** Sentence split around the bold app name "WhisperAround". */
    bodyBefore: "To paste the text automatically (⌘V) into the app you're using. Turn on",
    bodyAfter:
      "in System Settings › Privacy & Security › Accessibility. Without it, the text stays on the clipboard.",
    allow: "Allow Accessibility",
  },
  python: {
    /** Sentence split around the bold word (`neededBold`). */
    neededBefore: "",
    neededBold: "Local",
    neededAfter: "models (faster-whisper, no internet or API key) need Python.",
    /** Sentence split around the inline command `brew install python`. */
    installMacBefore: "Install it from python.org (or with",
    installMacAfter: ") and then check again.",
    installWindows: "Install it for free from the Microsoft Store and then check again.",
    openPythonOrg: "Open python.org",
    openMicrosoftStore: "Open Microsoft Store",
    checkAgain: "Check Again",
    afterInstall: "When the installation finishes, click “Check Again”.",
    ready: "All set to use local models.",
  },
  cloudNote: "Prefer the cloud? OpenAI models don't need Python, just your API key in Settings.",
  openSettings: "Open Settings",
  continue: "Continue",
} as const;
