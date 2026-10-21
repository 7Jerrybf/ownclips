// Highlight ranges inside a match recording, computed from its kill events
// (nothing is cut to disk): each kill keeps `pre` seconds before and `post`
// after, and kills whose ranges overlap merge into one multi-kill range.
import type { MatchSummary } from "./api";

export interface Highlight {
  match: MatchSummary;
  start: number;
  end: number;
  kills: number;
  /** Time of the first kill, a good thumbnail frame. */
  firstKill: number;
}

export function highlights(match: MatchSummary, pre: number, post: number): Highlight[] {
  const duration = match.info.duration ?? Infinity;
  const kills = match.info.events
    .filter((e) => e.kind === "kill")
    .map((e) => e.t)
    .sort((a, b) => a - b);
  const out: Highlight[] = [];
  for (const t of kills) {
    const start = Math.max(0, t - pre);
    const end = Math.min(duration, t + post);
    const last = out[out.length - 1];
    if (last && start <= last.end) {
      last.end = end;
      last.kills += 1;
    } else {
      out.push({ match, start, end, kills: 1, firstKill: t });
    }
  }
  return out;
}

export const killLabel = (n: number) => (n >= 5 ? "ACE" : `${n} 殺`);
