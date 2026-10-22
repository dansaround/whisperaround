/** Helpers shared by the Meetings page, the History page and the pill. */
import { t } from "./i18n/index.svelte";
import type { HistoryEntry, MeetingDateFormat, MeetingSegment, MeetingView } from "./types";

const pad = (n: number) => String(n).padStart(2, "0");

/** `mm:ss`, or `h:mm:ss` from one hour on. */
export function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  return h ? `${h}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}` : `${pad(Math.floor(s / 60))}:${pad(s % 60)}`;
}

/** `hh:mm:ss`, like the transcript lines the backend writes. */
export function longClock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
}

/** Local date as `YYYY-MM-DD`, `DD-MM-YYYY` or `MM-DD-YYYY`, as the settings choose. */
function dateOf(d: Date, format: MeetingDateFormat): string {
  const y = d.getFullYear();
  const m = pad(d.getMonth() + 1);
  const day = pad(d.getDate());
  return format === "dmy" ? `${day}-${m}-${y}` : format === "mdy" ? `${m}-${day}-${y}` : `${y}-${m}-${day}`;
}

/** Characters Windows and macOS refuse in file names, plus control characters. */
export const FORBIDDEN_NAME_CHARS = /[<>:"\/\\|?*\x00-\x1f]/g;

/** The name in file names and headings: the setting, or the translated default when blank. */
export function meetingName(name: string): string {
  return name.trim() || t("meetings.md.name");
}

/** `2026-10-22 16-30-05 Meeting.md` in local time. */
export function meetingFileName(ts: number, format: MeetingDateFormat, name: string): string {
  const d = new Date(ts);
  const time = `${pad(d.getHours())}-${pad(d.getMinutes())}-${pad(d.getSeconds())}`;
  return `${dateOf(d, format)} ${time} ${meetingName(name)}.md`;
}

/** `[hh:mm:ss] Me: text` for one live segment. */
export function segmentLine(seg: MeetingSegment): string {
  const who = seg.speaker === "mic" ? t("meetings.me") : seg.speaker === "system" ? t("meetings.others") : null;
  return `[${longClock(seg.startMs)}] ${who ? `${who}: ` : ""}${seg.text}`;
}

/** The Markdown file for a meeting: a history entry or the live/last view. */
export function meetingMarkdown(source: HistoryEntry | MeetingView, format: MeetingDateFormat, name: string): string {
  const live = "segments" in source;
  const ts = live ? source.startedTs : source.ts;
  const durationMs = live ? Math.max(0, ...source.segments.map((s) => s.endMs)) : source.durationMs;
  // Blank lines between segments, or Markdown would run them into one paragraph.
  const transcript = (live ? source.segments.map(segmentLine) : source.text.split("\n")).join("\n\n");
  const d = new Date(ts);
  return [
    `# ${dateOf(d, format)} ${meetingName(name)} ${pad(d.getHours())}:${pad(d.getMinutes())}`,
    "",
    `- ${t("meetings.md.duration", { time: clock(durationMs) })}`,
    `- ${t("meetings.md.model", { model: source.model })}`,
    "",
    transcript,
    "",
  ].join("\n");
}
