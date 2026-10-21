// Full-match timeline: kill markers above the track, deaths below,
// highlight ranges as bands on the track, the playhead, and an optional
// in/out selection. Click to seek.
import { useRef, useState } from "react";
import type { MatchEvent } from "../api";
import { clock, kindLabel } from "../format";

export function Timeline({
  duration,
  current,
  events,
  range,
  bands,
  onSeek,
}: {
  duration: number;
  current: number;
  events: MatchEvent[];
  range: [number | null, number | null];
  bands: { start: number; end: number; active: boolean }[];
  onSeek: (t: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<{ x: number; label: string } | null>(null);
  const pct = (t: number) => `${Math.min(100, Math.max(0, (t / Math.max(duration, 0.001)) * 100))}%`;

  const timeAt = (clientX: number) => {
    const r = ref.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * duration;
  };

  const [rin, rout] = range;
  return (
    <div
      ref={ref}
      className="timeline"
      onClick={(e) => onSeek(timeAt(e.clientX))}
      onMouseMove={(e) => {
        const r = ref.current!.getBoundingClientRect();
        setHover({ x: e.clientX - r.left, label: clock(timeAt(e.clientX)) });
      }}
      onMouseLeave={() => setHover(null)}
    >
      <div className="track">
        <div className="played" style={{ width: pct(current) }} />
        {bands.map((b, i) => (
          <div
            key={i}
            className={`band ${b.active ? "active" : ""}`}
            style={{ left: pct(b.start), width: `calc(${pct(b.end)} - ${pct(b.start)})` }}
          />
        ))}
        {rin !== null && (
          <div
            className="range"
            style={{ left: pct(rin), width: `calc(${pct(rout ?? current)} - ${pct(rin)})` }}
          />
        )}
      </div>
      {events.map((e, i) => (
        <div
          key={i}
          className={`marker ${e.kind}`}
          style={{ left: pct(e.t) }}
          onClick={(ev) => {
            ev.stopPropagation();
            onSeek(Math.max(0, e.t - 3));
          }}
          onMouseMove={(ev) => {
            ev.stopPropagation();
            const r = ref.current!.getBoundingClientRect();
            setHover({ x: ev.clientX - r.left, label: `${kindLabel[e.kind]} · ${clock(e.t)}` });
          }}
        />
      ))}
      <div className="playhead" style={{ left: pct(current) }} />
      {hover && (
        <div className="hover-tip num" style={{ left: hover.x }}>
          {hover.label}
        </div>
      )}
    </div>
  );
}
