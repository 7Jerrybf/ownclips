// One match: the full recording with a marked timeline, the event list,
// and tools to cut clips (automatic kill clips or a hand-picked range).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ClipSummary, EventKind, MatchSummary } from "../api";
import { api, fileUrl } from "../api";
import { clipAt, clock, count, kindLabel, shortDate } from "../format";
import { ClipModal } from "./ClipModal";
import { IconAssist, IconBack, IconDeath, IconKill, IconNext, IconPlay, IconPrev, IconScissors } from "./Icons";
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
  clips,
  onBack,
  onClipsChanged,
  notify,
}: {
  match: MatchSummary;
  clips: ClipSummary[];
  onBack: () => void;
  onClipsChanged: () => void;
  notify: (message: string, kind?: "error" | "success") => void;
}) {
  const video = useRef<HTMLVideoElement>(null);
  const [current, setCurrent] = useState(0);
  const [duration, setDuration] = useState(match.info.duration ?? 0);
  const [range, setRange] = useState<[number | null, number | null]>([null, null]);
  const [busy, setBusy] = useState(false);
  const [playing, setPlaying] = useState<ClipSummary | null>(null);

  const events = useMemo(() => [...match.info.events].sort((a, b) => a.t - b.t), [match]);
  const matchClips = clips.filter((c) => c.recording === match.name);

  // Smooth playhead while playing.
  useEffect(() => {
    let raf = 0;
    const loop = () => {
      if (video.current) setCurrent(video.current.currentTime);
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

  const jump = useCallback(
    (dir: 1 | -1) => {
      // The event we're "at" is the one whose lead-in we're inside.
      const now = current + LEAD_IN + 0.5 * dir;
      const target = dir > 0 ? events.find((e) => e.t > now) : [...events].reverse().find((e) => e.t < now - 1);
      if (target) seek(Math.max(0, target.t - LEAD_IN));
    },
    [current, events, seek],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (playing || tag === "INPUT") return;
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
  }, [duration, jump, seek, playing]);

  async function cutRange() {
    const [s, e] = range;
    if (s === null || e === null) return;
    setBusy(true);
    try {
      await api.cutRange(match.video, s, e);
      notify(`已剪下 ${clock(s)} – ${clock(e)}`, "success");
      setRange([null, null]);
      onClipsChanged();
    } catch (err) {
      notify(String(err), "error");
    }
    setBusy(false);
  }

  async function autoClips() {
    setBusy(true);
    try {
      const n = await api.clipMatch(match.video);
      notify(n > 0 ? `新增 ${n} 個擊殺片段` : "擊殺片段都已經裁過了", "success");
      onClipsChanged();
    } catch (err) {
      notify(String(err), "error");
    }
    setBusy(false);
  }

  const currentEvent = events.findIndex((e) => current >= e.t - LEAD_IN - 0.2 && current <= e.t + 2);
  const [rin, rout] = range;

  return (
    <div className="page">
      <button className="btn ghost back" onClick={onBack}>
        <IconBack size={16} /> 所有對局
      </button>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            {match.info.map} <span className="chip" style={{ verticalAlign: 4 }}>{match.info.mode}</span>
          </h1>
          <div className="page-sub">
            {shortDate(match.info.started_at)} · {clock(duration)}
          </div>
        </div>
      </div>

      <div className="viewer">
        <div>
          <div className="player-wrap">
            <video
              ref={video}
              src={fileUrl(match.video)}
              controls
              onLoadedMetadata={(e) => setDuration(e.currentTarget.duration)}
            />
          </div>

          <div className="timeline-panel">
            <Timeline duration={duration} current={current} events={events} range={range} onSeek={(t) => seek(t)} />
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
              </div>
            </div>
            <div className="timeline-foot" style={{ marginTop: 10 }}>
              <button className="btn sm" onClick={() => setRange([current, null])} title="設為起點 ( I )">
                設為起點 {rin !== null && <span className="num">{clock(rin)}</span>}
              </button>
              <button
                className="btn sm"
                onClick={() => setRange(([s]) => [s ?? 0, current])}
                title="設為終點 ( O )"
              >
                設為終點 {rout !== null && <span className="num">{clock(rout)}</span>}
              </button>
              <button
                className="btn sm primary"
                disabled={busy || rin === null || rout === null || rout <= rin}
                onClick={cutRange}
              >
                <IconScissors size={14} /> 剪下這段
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
              這場的片段
            </h2>
            <div className="page-actions">
              <button className="btn sm" disabled={busy} onClick={autoClips}>
                {busy ? <span className="spinner" /> : <IconScissors size={14} />} 裁出擊殺片段
              </button>
            </div>
          </div>
          {matchClips.length === 0 ? (
            <div className="empty" style={{ padding: 28 }}>
              還沒有片段。按「裁出擊殺片段」,或在時間軸上選一段剪下。
            </div>
          ) : (
            <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))" }}>
              {matchClips.map((c) => (
                <div key={c.path} className="card" onClick={() => setPlaying(c)}>
                  <div className="card-body" style={{ flexDirection: "row", alignItems: "center" }}>
                    <span className="ev-icon kill">
                      <IconPlay size={13} />
                    </span>
                    <span className="num">{clipAt(c.at)}</span>
                    <span className={`chip ${c.kills && c.kills >= 2 ? "gold" : ""}`} style={{ marginLeft: "auto" }}>
                      {c.label}
                    </span>
                  </div>
                </div>
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
                onClick={() => seek(Math.max(0, e.t - LEAD_IN))}
              >
                <EventIcon kind={e.kind} />
                {kindLabel[e.kind]}
                <span className="t num">{clock(e.t)}</span>
              </button>
            ))}
          </div>
        </div>
      </div>

      {playing && (
        <ClipModal
          path={playing.path}
          title={`${match.info.map} · ${clipAt(playing.at)} · ${playing.label}`}
          onClose={() => setPlaying(null)}
        />
      )}
    </div>
  );
}
