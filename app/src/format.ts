import type { EventKind, MatchEvent } from "./api";

/** 83.4 -> "1:23" (or "1:02:03" past an hour). */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

export function size(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  return `${Math.round(bytes / 1024 ** 2)} MB`;
}

export const kindLabel: Record<EventKind, string> = { kill: "擊殺", death: "死亡", assist: "助攻" };

export function count(events: MatchEvent[], kind: EventKind): number {
  return events.filter((e) => e.kind === kind).length;
}

/** "2026-10-21 19:29:38" -> "10/21 19:29". */
export function shortDate(startedAt: string): string {
  const m = startedAt.match(/^\d{4}-(\d{2})-(\d{2}) (\d{2}:\d{2})/);
  return m ? `${Number(m[1])}/${Number(m[2])} ${m[3]}` : startedAt;
}

/** "03m27s" -> "3:27". */
export function clipAt(at: string): string {
  const m = at.match(/^(\d+)m(\d+)s$/);
  return m ? `${Number(m[1])}:${m[2]}` : at;
}
