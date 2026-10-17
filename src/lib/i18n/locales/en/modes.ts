/** English strings: modes. Source of truth for every other locale. */
export default {
  title: "Modes",
  info: "A mode bundles a voice model and a language. The active one is used for every recording.",
  create: "Create mode",
  /** Default name given to a newly created mode. */
  newModeName: "New mode",
  activeMode: "Active mode",
  /** Group heading in the voice-model picker for on-device models. */
  localGroup: "Local",
  field: {
    name: "Name",
    icon: "Icon",
    voiceModel: "Voice model",
    language: "Language",
  },
  /** Accessible names for the icon choices. */
  icons: {
    sparkle: "Sparkle",
    message: "Message",
    laptop: "Laptop",
    pen: "Pen",
    code: "Code",
    globe: "Globe",
    bolt: "Bolt",
  },
  delete: "Delete",
  active: "Active",
  useThisMode: "Use this mode",
  tip: {
    title: "Switch modes on the fly",
    /** {keys} is replaced by the shortcut keycaps (or by `noShortcut`). Keep the {keys} marker. */
    body: "Press {keys} anywhere to jump to the next mode. The pill shows which one is active.",
    noShortcut: "your “Change mode” shortcut",
    dismiss: "Dismiss",
  },
} as const;
