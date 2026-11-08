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
  IconPause,
  IconPlay,
  IconVolume,
} from "./Icons";

export const SPEEDS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2];
export const SKIP = 5;
const IDLE_MS = 2500;

export function speedLabel(s: number) {
  return s === 1 ? "正常" : `${s}x`;
}

export function Player({
  videoRef,
  src,
  current,
  duration,
  speed,
  onSpeed,
  onSkip,
  timeline,
  navTools,
  editTools,
  onLoadedMetadata,
  onSeeking,
}: {
  videoRef: RefObject<HTMLVideoElement>;
  src: string;
  current: number;
  duration: number;
  speed: number;
  onSpeed: (s: number) => void;
  onSkip: (seconds: number) => void;
  /** The seek bar, full width across the top of the controls. */
  timeline: ReactNode;
  /** Buttons after the ±5 s ones (e.g. previous / next event). */
  navTools?: ReactNode;
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

  // "F" toggles fullscreen (the other shortcuts live in MatchDetail).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).tagName === "INPUT") return;
      if (e.key === "f" || e.key === "F") {
        e.preventDefault();
        toggleFullscreen();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleFullscreen]);

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

        <div className="player-row">
          <button className="player-btn" onClick={togglePlay} title={playing ? "暫停 (Space)" : "播放 (Space)"}>
            {playing ? <IconPause size={18} /> : <IconPlay size={18} />}
          </button>
          <button className="player-btn" onClick={() => onSkip(-SKIP)} title={`倒退 ${SKIP} 秒 (←)`}>
            <IconBack5 size={20} />
          </button>
          <button className="player-btn" onClick={() => onSkip(SKIP)} title={`快進 ${SKIP} 秒 (→)`}>
            <IconFwd5 size={20} />
          </button>
          {navTools}

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

          <button className="player-btn" onClick={toggleFullscreen} title={fullscreen ? "離開全螢幕 (F)" : "全螢幕 (F)"}>
            {fullscreen ? <IconExitFullscreen size={18} /> : <IconFullscreen size={18} />}
          </button>
        </div>
      </div>
    </div>
  );
}
