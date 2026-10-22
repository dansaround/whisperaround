/** Helpers shared by the Meetings page, the History page and the pill. */
import { t } from "./i18n/index.svelte";
import type { HistoryEntry, MeetingSegment, MeetingView } from "./types";

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

/** Local date as `YYYY-MM-DD`. */
function dateOf(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `YYYY-MM-DD HH-MM Meeting.md` in local time. */
export function meetingFileName(ts: number): string {
  const d = new Date(ts);
  return `${dateOf(d)} ${pad(d.getHours())}-${pad(d.getMinutes())} ${t("meetings.md.name")}.md`;
}

/** `[hh:mm:ss] Me: text` for one live segment. */
export function segmentLine(seg: MeetingSegment): string {
  const who = seg.speaker === "mic" ? t("meetings.me") : seg.speaker === "system" ? t("meetings.others") : null;
  return `[${longClock(seg.startMs)}] ${who ? `${who}: ` : ""}${seg.text}`;
}

/** The Markdown file for a meeting: a history entry or the live/last view. */
export function meetingMarkdown(source: HistoryEntry | MeetingView): string {
  const live = "segments" in source;
  const ts = live ? source.startedTs : source.ts;
  const durationMs = live ? Math.max(0, ...source.segments.map((s) => s.endMs)) : source.durationMs;
  // Blank lines between segments, or Markdown would run them into one paragraph.
  const transcript = (live ? source.segments.map(segmentLine) : source.text.split("\n")).join("\n\n");
  const d = new Date(ts);
  return [
    `# ${dateOf(d)} ${t("meetings.md.name")} ${pad(d.getHours())}:${pad(d.getMinutes())}`,
    "",
    `- ${t("meetings.md.duration", { time: clock(durationMs) })}`,
    `- ${t("meetings.md.model", { model: source.model })}`,
    "",
    transcript,
    "",
  ].join("\n");
}
