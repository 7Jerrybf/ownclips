// Cards grouped under a heading per day, and the date range filter above them.
import type { ReactNode } from "react";
import { type DateRange, dayLabel, groupByDay } from "../format";

const RANGES: { range: DateRange; label: string }[] = [
  { range: "all", label: "全部" },
  { range: "today", label: "今天" },
  { range: "week", label: "最近 7 天" },
  { range: "month", label: "本月" },
];

export function RangeFilter({ value, onChange }: { value: DateRange; onChange: (r: DateRange) => void }) {
  return (
    <div className="segmented">
      {RANGES.map((r) => (
        <button key={r.range} className={value === r.range ? "on" : ""} onClick={() => onChange(r.range)}>
          {r.label}
        </button>
      ))}
    </div>
  );
}

export function DayGroups<T>({
  items,
  startedAt,
  summary,
  children,
}: {
  items: T[];
  startedAt: (item: T) => string;
  /** Stats shown on the right of each day's heading. */
  summary: (items: T[]) => ReactNode;
  children: (item: T) => ReactNode;
}) {
  const now = new Date();
  return (
    <>
      {groupByDay(items, startedAt).map((g) => {
        const label = dayLabel(g.key, now);
        return (
          <section key={g.key} className="day-group">
            <div className="day-head">
              <b>{label.title}</b>
              <span>{label.sub}</span>
              <span className="day-summary num">{summary(g.items)}</span>
            </div>
            <div className="grid">{g.items.map(children)}</div>
          </section>
        );
      })}
    </>
  );
}
