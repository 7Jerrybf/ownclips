// One exported clip, played with the same on-video controls as a match.
import { useEffect, useRef, useState } from "react";
import type { ClipSummary } from "../api";
import { api, fileUrl } from "../api";
import { clock, shortDate, size, stampOf } from "../format";
import { clipTitle } from "./HighlightsView";
import { IconBack, IconFolder, IconMatches, IconTrash } from "./Icons";
import { Player } from "./Player";
import { Timeline } from "./Timeline";

export function ClipDetail({
  clip,
  onBack,
  onOpenMatch,
  notify,
}: {
  clip: ClipSummary;
  onBack: () => void;
  /** Opens the source match on this clip's range; absent when it's gone. */
  onOpenMatch?: (range: [number, number]) => void;
  notify: (message: string, kind?: "error" | "success") => void;
}) {
  const video = useRef<HTMLVideoElement>(null);
  const [current, setCurrent] = useState(0);
  const [duration, setDuration] = useState(0);
  const [confirm, setConfirm] = useState(false);

  // Smooth playhead.
  useEffect(() => {
    let raf = 0;
    const loop = () => {
      if (video.current) setCurrent(video.current.currentTime);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, []);

  async function remove() {
    try {
      await api.deleteClips([clip.path]);
      notify("已移到資源回收筒", "success");
      onBack();
    } catch (e) {
      notify(String(e), "error");
    }
  }

  const seek = (t: number) => {
    const v = video.current;
    if (!v) return;
    v.currentTime = t;
    void v.play();
  };
  const at = clip.at;

  return (
    <div className="page">
      <button className="btn ghost back" onClick={onBack}>
        <IconBack size={16} /> 返回
      </button>
      <div className="page-head">
        <div>
          <h1 className="page-title">
            {clipTitle(clip)}{" "}
            {clip.info && (
              <span className="chip" style={{ verticalAlign: 4 }}>
                {clip.info.mode}
              </span>
            )}
          </h1>
          <div className="page-sub num">
            {shortDate(stampOf(clip.modified_ms))} 匯出
            {at !== null && ` · 錄影 ${clock(at)} 起`} · {clock(duration)} · {size(clip.size)}
          </div>
        </div>
        <div className="page-actions">
          {onOpenMatch && at !== null && (
            <button className="btn" onClick={() => onOpenMatch([at, at + duration])} title="打開這個片段來源的整段錄影">
              <IconMatches size={16} /> 原始錄影
            </button>
          )}
          <button className="btn" onClick={() => api.reveal(clip.path)}>
            <IconFolder size={16} /> 在資料夾中顯示
          </button>
          {confirm ? (
            <>
              <button className="btn danger" onClick={remove}>
                確定刪除
              </button>
              <button className="btn ghost" onClick={() => setConfirm(false)}>
                取消
              </button>
            </>
          ) : (
            <button className="btn" onClick={() => setConfirm(true)}>
              <IconTrash size={16} /> 刪除
            </button>
          )}
        </div>
      </div>

      <Player
        videoRef={video}
        src={fileUrl(clip.path)}
        current={current}
        duration={duration}
        onLoadedMetadata={(e) => {
          setDuration(e.currentTarget.duration);
          void e.currentTarget.play().catch(() => {});
        }}
        onSeeking={() => {}}
        timeline={
          <Timeline duration={duration} current={current} events={[]} range={[null, null]} bands={[]} onSeek={seek} />
        }
      />
    </div>
  );
}
