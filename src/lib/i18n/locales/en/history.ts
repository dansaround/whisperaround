/** English strings: history. Source of truth for every other locale. */
export default {
  title: "History",
  saveHistory: "Save history",
  deleteAll: "Delete all",
  cancel: "Cancel",
  clear: "Clear",
  searchPlaceholder: "Search transcripts or apps",
  clearSearch: "Clear search",
  today: "Today",
  yesterday: "Yesterday",
  seconds: "{s}s",
  minutesSeconds: "{m}m {s}s",
  words: {
    one: "{count} word",
    other: "{count} words",
  },
  copy: "Copy",
  delete: "Delete",
  copyFailed: "Couldn't copy to the clipboard",
  empty: {
    noMatches: "No matches",
    noMatchesSub: "Nothing in your history mentions “{query}”.",
    off: "History is off",
    offSub: "Turn on “Save history” to keep your transcripts here.",
    none: "No transcripts yet",
    noneSub: "Your dictations will show up here, newest first.",
  },
} as const;
