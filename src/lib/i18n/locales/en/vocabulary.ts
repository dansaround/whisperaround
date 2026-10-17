/** English strings: vocabulary. Source of truth for every other locale. */
export default {
  title: "Vocabulary",
  info: "Whisper uses these terms as a hint, so names, brands and jargon come out spelled the way you write them.",
  count: { one: "{count} term", other: "{count} terms" },
  placeholder: "Add a word, name or phrase — paste a list to add many",
  add: "Add",
  remove: "Remove {word}",
  empty: {
    title: "Teach WhisperAround your words",
    sub: "Names, products, acronyms and industry terms. Try:",
  },
  footnote:
    "Terms are sent as a spelling hint with every transcription (cloud and local models). Keep the list focused — a few dozen terms work best.",
} as const;
