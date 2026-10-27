// One match: the full recording with a marked timeline, the event list,
// and its highlight ranges (played straight from the recording; a range can
// be exported to a file on request).
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EventKind, MatchSummary } from "../api";
import { api, fileUrl } from "../api";
import { clock, count, gameLabel, kindLabel, shortDate } from "../format";
import { type Highlight, highlights, killLabel } from "../highlights";
import {
  IconAssist,
  IconBack,
  IconDeath,
  IconFolder,
  IconKeep,
  IconKill,
  IconNext,
  IconPlay,
  IconPrev,
  IconScissors,
} from "./Icons";
import { Timeline } from "./Timeline";

const LEAD_IN = 3;

export function EventIcon({ kind }: { kind: EventKind }) {
  const Icon = kind === "kill" ? IconKill : kind === "death" ? IconDeath : IconAssist;
  return (
    <span className={`ev-icon ${kind}`}>
      <Icon size={15} />
    </span>
  );
}

export function MatchDetail({
  match,
  pre,
  post,
  initialRange,
  onBack,
  notify,
}: {
  match: MatchSummary;
  pre: number;
  post: number;
  /** Start playing this range right away (opened from the highlights page). */
  initialRange?: [number, number];
  onBack: () => void;
  notify: (message: string, kind?: "error" | "success") => void;
}) {
  const video = useRef<HTMLVideoElement>(null);
  const [current, setCurrent] = useState(0);
  const [reanalyzing, setReanalyzing] = useState<number | null>(null);

  useEffect(() => {
    const unlisten = listen<[string, number]>("reanalyze-progress", (e) => {
      if (e.payload[0] === match.video) setReanalyzing(e.payload[1]);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [match.video]);

  async function reanalyze() {
    setReanalyzing(0);
    try {
      await api.reanalyze(match.video);
      notify("已重新偵測擊殺 / 死亡 / 助攻", "success");
    } catch (e) {
      notify(String(e), "error");
    }
    setReanalyzing(null);
  }
  const [duration, setDuration] = useState(match.info.duration ?? 0);
  const [range, setRange] = useState<[number | null, number | null]>([null, null]);
  const [busy, setBusy] = useState(false);
  /** Range being played; playback pauses at its end. */
  const playingRange = useRef<[number, number] | null>(initialRange ?? null);

  const events = useMemo(() => [...match.info.events].sort((a, b) => a.t - b.t), [match]);
  const ranges = useMemo(() => highlights(match, pre, post), [match, pre, post]);

  // Smooth playhead while playing; stop at the end of a highlight range.
  useEffect(() => {
    let raf = 0;
    const loop = () => {
      const v = video.current;
      if (v) {
        setCurrent(v.currentTime);
        const r = playingRange.current;
        if (r && v.currentTime >= r[1]) {
          v.pause();
          playingRange.current = null;
        }
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, []);

  const seek = useCallback((t: number, play = true) => {
    const v = video.current;
    if (!v) return;
    v.currentTime = t;
    if (play) void v.play();
  }, []);

  const playHighlight = useCallback(
    (h: { start: number; end: number }) => {
      playingRange.current = [h.start, h.end];
      seek(h.start);
    },
    [seek],
  );

  const jump = useCallback(
    (dir: 1 | -1) => {
      // The event we're "at" is the one whose lead-in we're inside.
      const now = current + LEAD_IN + 0.5 * dir;
      const target = dir > 0 ? events.find((e) => e.t > now) : [...events].reverse().find((e) => e.t < now - 1);
      if (target) {
        playingRange.current = null;
        seek(Math.max(0, target.t - LEAD_IN));
      }
    },
    [current, events, seek],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT") return;
      const v = video.current;
      if (!v) return;
      // The focused <video> already handles Space itself.
      if (e.key === " " && (tag === "VIDEO" || tag === "BUTTON")) return;
      if (e.key === "ArrowRight") seek(Math.min(duration, v.currentTime + 5), !v.paused);
      else if (e.key === "ArrowLeft") seek(Math.max(0, v.currentTime - 5), !v.paused);
      else if (e.key === ".") jump(1);
      else if (e.key === ",") jump(-1);
      else if (e.key === "i") setRange([v.currentTime, null]);
      else if (e.key === "o") setRange(([s]) => [s ?? 0, v.currentTime]);
      else if (e.key === " ") v.paused ? void v.play() : v.pause();
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [duration, jump, seek]);

  async function exportRange(start: number, end: number) {
    setBusy(true);
    try {
      const path = await api.cutRange(match.video, start, end);
      notify(`已匯出 ${clock(start)} – ${clock(end)}`, "success");
      void api.reveal(path);
      setRange([null, null]);
    } catch (err) {
      notify(String(err), "error");
    }
    setBusy(false);
  }

  const currentEvent = events.findIndex((e) => current >= e.t - LEAD_IN - 0.2 && current <= e.t + 2);
  const activeHighlight = ranges.findIndex((h) => current >= h.start && current <= h.end);
  const [rin, rout] = range;

  return (
    <div className="page">
      <button className="btn ghost back" onClick={onBack}>
        <IconBack size={16} /> 返回
      </button>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            {match.info.map}{" "}
            <span className="chip" style={{ verticalAlign: 4 }}>
              {match.info.mode}
            </span>
          </h1>
          <div className="page-sub">
            {gameLabel[match.info.game]} · {shortDate(match.info.started_at)} · {clock(duration)}
          </div>
        </div>
        <div className="page-actions">
          <button
            className={`btn ${match.info.keep ? "keep-on" : ""}`}
            disabled={match.recording}
            onClick={() => api.setKeep(match.video, !match.info.keep).catch((e) => notify(String(e), "error"))}
            title="保留的對局超過容量上限時不會被自動刪除"
          >
            <IconKeep size={16} filled={match.info.keep} /> {match.info.keep ? "已保留" : "保留這場"}
          </button>
          {match.info.game === "valorant" && (
            <button
              className="btn"
              disabled={reanalyzing !== null || match.recording}
              onClick={reanalyze}
              title="用最新的偵測方式重新掃描整場錄影,補上漏掉的擊殺 / 死亡 / 助攻"
            >
              {reanalyzing === null ? "重新偵測 KDA" : `偵測中… ${Math.round(reanalyzing * 100)}%`}
            </button>
          )}
        </div>
      </div>

      <div className="viewer">
        <div>
          <div className="player-wrap">
            <video
              ref={video}
              src={fileUrl(match.video)}
              controls
              onLoadedMetadata={(e) => {
                setDuration(e.currentTarget.duration);
                if (initialRange) seek(initialRange[0]);
              }}
              onSeeking={(e) => {
                // A manual seek outside the range ends "play this highlight".
                const t = e.currentTarget.currentTime;
                const r = playingRange.current;
                if (r && (t < r[0] - 0.5 || t > r[1])) playingRange.current = null;
              }}
            />
          </div>

          <div className="timeline-panel">
            <Timeline
              duration={duration}
              current={current}
              events={events}
              range={range}
              bands={ranges.map((h, i) => ({ start: h.start, end: h.end, active: i === activeHighlight }))}
              onSeek={(t) => {
                playingRange.current = null;
                seek(t);
              }}
            />
            <div className="timeline-foot">
              <button className="btn sm" onClick={() => jump(-1)} title="上一個事件 ( , )">
                <IconPrev size={14} /> 上一個
              </button>
              <button className="btn sm" onClick={() => jump(1)} title="下一個事件 ( . )">
                下一個 <IconNext size={14} />
              </button>
              <span className="num" style={{ margin: "0 6px", color: "var(--muted)" }}>
                {clock(current)} / {clock(duration)}
              </span>
              <div className="legend">
                <span>
                  <i style={{ background: "var(--kill)" }} />
                  擊殺
                </span>
                <span>
                  <i style={{ background: "var(--death)" }} />
                  死亡
                </span>
                <span>
                  <i style={{ background: "var(--assist)" }} />
                  助攻
                </span>
                <span>
                  <i style={{ background: "rgba(255, 201, 64, 0.35)" }} />
                  精華
                </span>
              </div>
            </div>
            <div className="timeline-foot" style={{ marginTop: 10 }}>
              <button className="btn sm" onClick={() => setRange([current, null])} title="設為起點 ( I )">
                設為起點 {rin !== null && <span className="num">{clock(rin)}</span>}
              </button>
              <button className="btn sm" onClick={() => setRange(([s]) => [s ?? 0, current])} title="設為終點 ( O )">
                設為終點 {rout !== null && <span className="num">{clock(rout)}</span>}
              </button>
              <button
                className="btn sm"
                disabled={busy || rin === null || rout === null || rout <= rin}
                onClick={() => rin !== null && rout !== null && exportRange(rin, rout)}
              >
                <IconScissors size={14} /> 匯出這段
              </button>
              {rin !== null && (
                <button className="btn sm ghost" onClick={() => setRange([null, null])}>
                  清除
                </button>
              )}
              <span style={{ marginLeft: "auto", color: "var(--faint)", fontSize: 12 }}>
                <span className="kbd">Space</span>播放 <span className="kbd">←</span>
                <span className="kbd">→</span>5 秒 <span className="kbd">,</span>
                <span className="kbd">.</span>跳事件 <span className="kbd">I</span>
                <span className="kbd">O</span>起終點
              </span>
            </div>
          </div>

          <div className="page-head" style={{ marginTop: 28, marginBottom: 14 }}>
            <h2 className="page-title" style={{ fontSize: 18 }}>
              精華區間 · {ranges.length}
            </h2>
          </div>
          {ranges.length === 0 ? (
            <div className="empty" style={{ padding: 28 }}>
              這場沒有擊殺,所以沒有精華區間。
            </div>
          ) : (
            <div className="highlight-list">
              {ranges.map((h, i) => (
                <HighlightRow
                  key={i}
                  h={h}
                  active={i === activeHighlight}
                  busy={busy}
                  onPlay={() => playHighlight(h)}
                  onExport={() => exportRange(h.start, h.end)}
                />
              ))}
            </div>
          )}
        </div>

        <div className="side-panel">
          <div className="stat-row">
            <div className="stat">
              <b className="num" style={{ color: "var(--kill)" }}>
                {count(events, "kill")}
              </b>
              <span>擊殺</span>
            </div>
            <div className="stat">
              <b className="num" style={{ color: "var(--death)" }}>
                {count(events, "death")}
              </b>
              <span>死亡</span>
            </div>
            <div className="stat">
              <b className="num" style={{ color: "var(--assist)" }}>
                {count(events, "assist")}
              </b>
              <span>助攻</span>
            </div>
          </div>
          <h3>事件 · {events.length}</h3>
          <div className="event-list">
            {events.length === 0 && <div style={{ padding: 16, color: "var(--muted)" }}>這場沒有偵測到事件</div>}
            {events.map((e, i) => (
              <button
                key={i}
                className={`event-item ${i === currentEvent ? "current" : ""}`}
                onClick={() => {
                  playingRange.current = null;
                  seek(Math.max(0, e.t - LEAD_IN));
                }}
              >
                <EventIcon kind={e.kind} />
                {kindLabel[e.kind]}
                <span className="t num">{clock(e.t)}</span>
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function HighlightRow({
  h,
  active,
  busy,
  onPlay,
  onExport,
}: {
  h: Highlight;
  active: boolean;
  busy: boolean;
  onPlay: () => void;
  onExport: () => void;
}) {
  return (
    <div className={`highlight-row ${active ? "active" : ""}`} onClick={onPlay}>
      <span className="ev-icon kill">
        <IconPlay size={13} />
      </span>
      <span className="num">
        {clock(h.start)} – {clock(h.end)}
      </span>
      <span className={`chip ${h.kills >= 2 ? "gold" : "kill"}`}>{killLabel(h.kills)}</span>
      <span className="card-meta num">{Math.round(h.end - h.start)} 秒</span>
      <button
        className="btn sm ghost"
        style={{ marginLeft: "auto" }}
        disabled={busy}
        title="把這段存成獨立的影片檔"
        onClick={(e) => {
          e.stopPropagation();
          onExport();
        }}
      >
        <IconFolder size={14} /> 匯出
      </button>
    </div>
  );
}
