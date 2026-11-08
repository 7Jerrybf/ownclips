// The recording player with our own control bar laid over the video: the
// match timeline, play, ±5 s, speed, volume, fullscreen, plus whatever
// tools the page adds (event jumps, clip in/out). Fullscreen goes on the
// wrapper rather than the <video>, so all of it stays usable there.
import type { CSSProperties, ReactEventHandler, ReactNode, RefObject } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { clock } from "../format";
import {
  IconBack5,
  IconExitFullscreen,
  IconFullscreen,
  IconFwd5,
  IconMute,
  IconNext,
  IconPause,
  IconPlay,
  IconPrev,
  IconVolume,
} from "./Icons";

export const SPEEDS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2];
export const SKIP = 5;
const IDLE_MS = 2500;

export function speedLabel(s: number) {
  return `${s}x`;
}

export function Player({
  videoRef,
  src,
  current,
  duration,
  timeline,
  onEvent,
  editTools,
  onLoadedMetadata,
  onSeeking,
}: {
  videoRef: RefObject<HTMLVideoElement>;
  src: string;
  current: number;
  duration: number;
  /** The seek bar, full width across the top of the controls. */
  timeline: ReactNode;
  /** Jump to the previous (-1) / next (1) event; adds those buttons. */
  onEvent?: (dir: 1 | -1) => void;
  /** Buttons before the speed menu (e.g. clip in / out / export). */
  editTools?: ReactNode;
  onLoadedMetadata: ReactEventHandler<HTMLVideoElement>;
  onSeeking: ReactEventHandler<HTMLVideoElement>;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const [playing, setPlaying] = useState(false);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [idle, setIdle] = useState(false);
  const [speedMenu, setSpeedMenu] = useState(false);
  /** A button is held down in the controls (e.g. dragging a timeline handle). */
  const [holding, setHolding] = useState(false);
  const idleTimer = useRef(0);
  const [speed, setSpeed] = useState(() => {
    try {
      const s = Number(localStorage.getItem("playback-speed"));
      return SPEEDS.includes(s) ? s : 1;
    } catch {
      return 1;
    }
  });
  const onSpeed = useCallback((s: number) => {
    setSpeed(s);
    try {
      localStorage.setItem("playback-speed", String(s));
    } catch {
      // Not remembered; fine.
    }
  }, []);
  const onSkip = useCallback(
    (seconds: number) => {
      const v = videoRef.current;
      if (!v) return;
      const end = Number.isFinite(v.duration) ? v.duration : duration;
      v.currentTime = Math.min(end, Math.max(0, v.currentTime + seconds));
    },
    [duration, videoRef],
  );

  useEffect(() => {
    const v = videoRef.current;
    if (v) {
      v.playbackRate = speed;
      v.defaultPlaybackRate = speed;
    }
  }, [speed, videoRef]);

  useEffect(() => {
    const onChange = () => setFullscreen(document.fullscreenElement === wrap.current);
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);

  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen();
    else void wrap.current?.requestFullscreen();
  }, []);

  // Player shortcuts; pages add their own (event jumps, clip in/out).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || e.ctrlKey || e.altKey || e.metaKey) return;
      const v = videoRef.current;
      if (!v) return;
      // A focused button already handles Space itself.
      if (e.key === " " && tag === "BUTTON") return;
      if (e.key === "f" || e.key === "F") toggleFullscreen();
      else if (e.key === " ") v.paused ? void v.play() : v.pause();
      else if (e.key === "ArrowRight") onSkip(SKIP);
      else if (e.key === "ArrowLeft") onSkip(-SKIP);
      else if (e.key === ">") onSpeed(SPEEDS[Math.min(SPEEDS.length - 1, SPEEDS.indexOf(speed) + 1)]);
      else if (e.key === "<") onSpeed(SPEEDS[Math.max(0, SPEEDS.indexOf(speed) - 1)]);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleFullscreen, onSkip, onSpeed, speed, videoRef]);

  const wake = useCallback(() => {
    setIdle(false);
    window.clearTimeout(idleTimer.current);
    idleTimer.current = window.setTimeout(() => setIdle(true), IDLE_MS);
  }, []);
  useEffect(() => () => window.clearTimeout(idleTimer.current), []);

  const togglePlay = () => {
    const v = videoRef.current;
    if (!v) return;
    v.paused ? void v.play() : v.pause();
  };

  const hidden = playing && idle && !speedMenu && !holding;
  const vol = muted ? 0 : volume;

  return (
    <div
      ref={wrap}
      className={`player-wrap ${fullscreen ? "fullscreen" : ""} ${hidden ? "idle" : ""}`}
      onMouseMove={wake}
      onMouseLeave={() => playing && setIdle(true)}
    >
      <video
        ref={videoRef}
        src={src}
        onClick={() => {
          if (speedMenu) setSpeedMenu(false);
          else togglePlay();
        }}
        onDoubleClick={toggleFullscreen}
        onPlay={() => {
          setPlaying(true);
          wake();
        }}
        onPause={() => setPlaying(false)}
        onVolumeChange={(e) => {
          setVolume(e.currentTarget.volume);
          setMuted(e.currentTarget.muted);
        }}
        onLoadedMetadata={(e) => {
          // A new source resets the rate to the default; keep the chosen one.
          e.currentTarget.playbackRate = speed;
          onLoadedMetadata(e);
        }}
        onSeeking={onSeeking}
      />

      {/* Keep focus off the controls so Space / arrows stay player shortcuts. */}
      <div
        className="player-controls"
        onMouseDown={(e) => e.target instanceof HTMLInputElement || e.preventDefault()}
        onPointerDown={() => setHolding(true)}
        onPointerUp={() => setHolding(false)}
        onPointerCancel={() => setHolding(false)}
        onLostPointerCapture={() => setHolding(false)}
      >
        <div className="player-timeline">{timeline}</div>

        {/* Volume and time | transport, centred | clip tools and view. */}
        <div className="player-row">
          <div className="player-side">
            <div className="player-volume">
              <button
                className="player-btn"
                onClick={() => {
                  const v = videoRef.current;
                  if (!v) return;
                  v.muted = !v.muted;
                  if (!v.muted && v.volume === 0) v.volume = 1;
                }}
                title={muted ? "取消靜音" : "靜音"}
              >
                {vol === 0 ? <IconMute size={18} /> : <IconVolume size={18} />}
              </button>
              <input
                type="range"
                min={0}
                max={1}
                step={0.05}
                value={vol}
                style={{ "--pct": `${vol * 100}%` } as CSSProperties}
                onChange={(e) => {
                  const v = videoRef.current;
                  if (!v) return;
                  v.volume = Number(e.currentTarget.value);
                  v.muted = v.volume === 0;
                }}
                onMouseUp={(e) => e.currentTarget.blur()}
              />
            </div>
            <span className="player-time num">
              {clock(current)} / {clock(duration)}
            </span>
          </div>

          <div className="player-transport">
            {onEvent && (
              <button className="player-btn" onClick={() => onEvent(-1)} title="上一個事件 ( , )">
                <IconPrev size={18} />
              </button>
            )}
            <button className="player-btn" onClick={() => onSkip(-SKIP)} title={`倒退 ${SKIP} 秒 (←)`}>
              <IconBack5 size={20} />
            </button>
            <button className="player-btn play" onClick={togglePlay} title={playing ? "暫停 (Space)" : "播放 (Space)"}>
              {playing ? <IconPause size={22} /> : <IconPlay size={22} />}
            </button>
            <button className="player-btn" onClick={() => onSkip(SKIP)} title={`快進 ${SKIP} 秒 (→)`}>
              <IconFwd5 size={20} />
            </button>
            {onEvent && (
              <button className="player-btn" onClick={() => onEvent(1)} title="下一個事件 ( . )">
                <IconNext size={18} />
              </button>
            )}
          </div>

          <div className="player-side end">
            <div className="player-tools">{editTools}</div>

            <div className="player-speed">
              <button
                className={`player-btn text ${speed !== 1 ? "on" : ""}`}
                onClick={() => setSpeedMenu((o) => !o)}
                title="播放速度 (Shift + < / >)"
              >
                {speed === 1 ? "1x" : `${speed}x`}
              </button>
              {speedMenu && (
                <div className="player-menu">
                  {[...SPEEDS].reverse().map((s) => (
                    <button
                      key={s}
                      className={s === speed ? "on" : ""}
                      onClick={() => {
                        onSpeed(s);
                        setSpeedMenu(false);
                      }}
                    >
                      {speedLabel(s)}
                    </button>
                  ))}
                </div>
              )}
            </div>

            <button
              className="player-btn"
              onClick={toggleFullscreen}
              title={fullscreen ? "離開全螢幕 (F)" : "全螢幕 (F)"}
            >
              {fullscreen ? <IconExitFullscreen size={18} /> : <IconFullscreen size={18} />}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
