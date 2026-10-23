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

const WEEKDAYS = ["週日", "週一", "週二", "週三", "週四", "週五", "週六"];
const pad = (n: number) => String(n).padStart(2, "0");
const localKey = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** "2026-10-21 19:29:38" -> "2026-10-21", the (local) day a match started. */
export const dayKey = (startedAt: string) => startedAt.slice(0, 10);

/** "2026-10-21 19:29:38" -> "19:29". */
export const timeOf = (startedAt: string) => startedAt.slice(11, 16) || startedAt;

function parseKey(key: string): Date | null {
  const m = key.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  return m ? new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])) : null;
}

/** Whole days from the given day to today (0 = today). */
function daysAgo(key: string, now: Date): number {
  const day = parseKey(key);
  if (!day) return NaN;
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.round((today.getTime() - day.getTime()) / 86_400_000);
}

/** Heading for a day: "今天" / "昨天" / "10月20日", with the weekday. */
export function dayLabel(key: string, now = new Date()): { title: string; sub: string } {
  const day = parseKey(key);
  if (!day) return { title: key, sub: "" };
  const weekday = WEEKDAYS[day.getDay()];
  const date = `${day.getFullYear() === now.getFullYear() ? "" : `${day.getFullYear()}年`}${day.getMonth() + 1}月${day.getDate()}日`;
  const ago = daysAgo(key, now);
  if (ago === 0) return { title: "今天", sub: `${date} ${weekday}` };
  if (ago === 1) return { title: "昨天", sub: `${date} ${weekday}` };
  return { title: date, sub: weekday };
}

export type DateRange = "all" | "today" | "week" | "month";

export function inRange(startedAt: string, range: DateRange, now = new Date()): boolean {
  const key = dayKey(startedAt);
  switch (range) {
    case "all":
      return true;
    case "month":
      return key.slice(0, 7) === localKey(now).slice(0, 7);
    case "today":
      return daysAgo(key, now) <= 0;
    case "week":
      return daysAgo(key, now) < 7;
  }
}

/** Items grouped by the day they started, newest day first, order kept within a day. */
export function groupByDay<T>(items: T[], startedAt: (item: T) => string): { key: string; items: T[] }[] {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const key = dayKey(startedAt(item));
    groups.set(key, [...(groups.get(key) ?? []), item]);
  }
  return [...groups].sort(([a], [b]) => b.localeCompare(a)).map(([key, items]) => ({ key, items }));
}
