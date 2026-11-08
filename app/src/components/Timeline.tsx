// Full-match timeline: kill markers above the track, deaths below,
// highlight ranges as bands on the track, the playhead, and an optional
// in/out selection. Click to seek; drag the in/out handles (or the selection
// itself) to adjust it.
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
  onRangeChange,
}: {
  duration: number;
  current: number;
  events: MatchEvent[];
  range: [number | null, number | null];
  bands: { start: number; end: number; active: boolean }[];
  onSeek: (t: number) => void;
  /** Called while dragging; `t` is the edge being moved (for a frame preview). */
  onRangeChange?: (range: [number, number | null], t: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<{ x: number; label: string } | null>(null);
  const pct = (t: number) => `${Math.min(100, Math.max(0, (t / Math.max(duration, 0.001)) * 100))}%`;

  const timeAt = (clientX: number) => {
    const r = ref.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (clientX - r.left) / r.width)) * duration;
  };

  const [rin, rout] = range;
  const [dragging, setDragging] = useState<"in" | "out" | "move" | null>(null);
  /** Shortest selection a drag may produce, in seconds. */
  const MIN_LEN = 0.5;

  const startDrag = (e: React.PointerEvent, which: "in" | "out" | "move") => {
    if (!onRangeChange || e.button !== 0 || duration <= 0) return;
    e.stopPropagation();
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    setDragging(which);
    // Unset edges start at the ends of the video.
    const start = { x: timeAt(e.clientX), rin: rin ?? 0, rout: rout ?? duration };
    const r = ref.current!.getBoundingClientRect();

    const move = (ev: PointerEvent) => {
      const t = timeAt(ev.clientX);
      setHover({ x: Math.min(r.width, Math.max(0, ev.clientX - r.left)), label: clock(t) });
      if (which === "in") {
        const v = Math.min(Math.max(0, t), Math.max(0, start.rout - MIN_LEN));
        onRangeChange([v, start.rout], v);
      } else if (which === "out") {
        const v = Math.max(Math.min(duration, t), Math.min(duration, start.rin + MIN_LEN));
        onRangeChange([start.rin, v], v);
      } else {
        const len = start.rout - start.rin;
        const v = Math.min(Math.max(0, start.rin + (t - start.x)), Math.max(0, duration - len));
        onRangeChange([v, v + len], v);
      }
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      setDragging(null);
      setHover(null);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };
  // A drag ends in a click on the handle; don't let it seek the timeline.
  const swallow = (e: React.MouseEvent) => e.stopPropagation();

  return (
    <div
      ref={ref}
      className={`timeline ${dragging ? "dragging" : ""}`}
      onClick={(e) => onSeek(timeAt(e.clientX))}
      onMouseMove={(e) => {
        if (dragging) return;
        const r = ref.current!.getBoundingClientRect();
        setHover({ x: e.clientX - r.left, label: clock(timeAt(e.clientX)) });
      }}
      onMouseLeave={() => !dragging && setHover(null)}
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
      {rin !== null && rout !== null && onRangeChange && (
        <div
          className="range-grab"
          style={{ left: pct(rin), width: `calc(${pct(rout)} - ${pct(rin)})` }}
          title="拖曳移動整段"
          onPointerDown={(e) => startDrag(e, "move")}
          onClick={swallow}
        />
      )}
      {onRangeChange && duration > 0 && (
        <div
          className={`range-handle in ${dragging === "in" ? "active" : ""} ${rin === null ? "idle" : ""}`}
          style={{ left: pct(rin ?? 0) }}
          title={rin === null ? "拖曳設定起點" : `起點 ${clock(rin)} · 拖曳調整`}
          onPointerDown={(e) => startDrag(e, "in")}
          onClick={swallow}
          onMouseMove={swallow}
        />
      )}
      {onRangeChange && duration > 0 && (
        <div
          className={`range-handle out ${dragging === "out" ? "active" : ""} ${rout === null ? "idle" : ""}`}
          style={{ left: pct(rout ?? duration) }}
          title={rout === null ? "拖曳設定終點" : `終點 ${clock(rout)} · 拖曳調整`}
          onPointerDown={(e) => startDrag(e, "out")}
          onClick={swallow}
          onMouseMove={swallow}
        />
      )}
      {hover && (
        <div className="hover-tip num" style={{ left: hover.x }}>
          {hover.label}
        </div>
      )}
    </div>
  );
}
