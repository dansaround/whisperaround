/** English strings: models. Source of truth for every other locale. */
export default {
  openai: {
    apiKey: "API key",
    configured: "Configured",
    missing: "Missing",
    keySub: "Stored in the system keychain and only ever sent to api.openai.com.",
    /** Windows variants: the key lives in Credential Manager (use its localized name). */
    keySubWindows: "Stored in Windows Credential Manager and only ever sent to api.openai.com.",
    replacePlaceholder: "••••••••••••  (replace)",
    saving: "Saving…",
    save: "Save",
    keySaved: "API key saved to the keychain",
    keySavedWindows: "API key saved to Credential Manager",
  },
  activeNote: "Picking a model sets it for your active mode",
  groups: {
    cloud: "Cloud",
    cloudSub: "OpenAI · needs an API key",
    local: "Local",
    localSub: "On your computer · private, offline",
  },
  python: {
    checking: "Checking Python…",
    notFound: "Python 3 not found",
    getPython: "Get Python",
    microsoftStore: "Microsoft Store",
  },
  card: {
    speed: "Speed",
    accuracy: "Accuracy",
    preparingEngine: "Preparing engine…",
    progress: "{received} of {total}",
    cancel: "Cancel",
    offline: "Offline",
    cloud: "Cloud",
    download: "Download",
    active: "Active",
    use: "Use",
    ready: "{model} is ready",
  },
} as const;
