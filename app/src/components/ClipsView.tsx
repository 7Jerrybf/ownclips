import { useMemo, useState } from "react";
import type { ClipSummary, MatchSummary } from "../api";
import { api, fileUrl } from "../api";
import { clipAt, size } from "../format";
import { ClipModal } from "./ClipModal";
import { IconClips, IconFolder, IconPlay, IconTrash } from "./Icons";
import { Thumb } from "./Thumb";

type Filter = "all" | "multi" | "custom";

export function ClipsView({
  clips,
  matches,
  onChanged,
  notify,
}: {
  clips: ClipSummary[];
  matches: MatchSummary[];
  onChanged: () => void;
  notify: (message: string, kind?: "error" | "success") => void;
}) {
  const [filter, setFilter] = useState<Filter>("all");
  const [hovered, setHovered] = useState<string | null>(null);
  const [playing, setPlaying] = useState<ClipSummary | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);

  const matchByName = useMemo(() => new Map(matches.map((m) => [m.name, m])), [matches]);
  const shown = clips.filter((c) =>
    filter === "all" ? true : filter === "multi" ? (c.kills ?? 0) >= 2 : c.kills === null,
  );
  const multi = clips.filter((c) => (c.kills ?? 0) >= 2).length;

  async function remove(c: ClipSummary) {
    try {
      await api.deleteClip(c.path);
      notify("已移到資源回收筒", "success");
      onChanged();
    } catch (e) {
      notify(String(e), "error");
    }
    setConfirm(null);
  }

  const title = (c: ClipSummary) => {
    const m = matchByName.get(c.recording);
    return `${m ? m.info.map + " · " : ""}${clipAt(c.at)} · ${c.label}`;
  };

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1 className="page-title">片段</h1>
          <div className="page-sub">
            {clips.length} 個片段 · {multi} 個多殺 · {size(clips.reduce((n, c) => n + c.size, 0))}
          </div>
        </div>
        <div className="page-actions">
          <button className="btn" onClick={() => api.recordingsDir().then((d) => api.openPath(d + "\\clips"))}>
            <IconFolder size={16} /> 片段資料夾
          </button>
        </div>
      </div>

      <div className="filter-row">
        <div className="segmented">
          <button className={filter === "all" ? "on" : ""} onClick={() => setFilter("all")}>
            全部
          </button>
          <button className={filter === "multi" ? "on" : ""} onClick={() => setFilter("multi")}>
            多殺
          </button>
          <button className={filter === "custom" ? "on" : ""} onClick={() => setFilter("custom")}>
            精選
          </button>
        </div>
      </div>

      {shown.length === 0 ? (
        <div className="empty">
          <IconClips size={34} />
          <h3>{clips.length === 0 ? "還沒有片段" : "沒有符合的片段"}</h3>
          <div>對局結束後會自動裁出擊殺片段;也可以在對局頁的時間軸上自己剪。</div>
        </div>
      ) : (
        <div className="grid">
          {shown.map((c) => {
            const m = matchByName.get(c.recording);
            const isMulti = (c.kills ?? 0) >= 2;
            return (
              <div
                key={c.path}
                className="card"
                onClick={() => setPlaying(c)}
                onMouseEnter={() => setHovered(c.path)}
                onMouseLeave={() => setHovered(null)}
              >
                <div className="thumb">
                  <Thumb path={c.path} at={-3.5} />
                  {hovered === c.path && <video src={fileUrl(c.path)} muted autoPlay loop style={{ position: "absolute", inset: 0 }} />}
                  <div className="shade" />
                  <div className="badge-tl">
                    <span className={`chip ${isMulti ? "gold" : c.kills ? "kill" : "assist"}`}>{c.label}</span>
                  </div>
                  <div className="corner">
                    <span style={{ fontWeight: 700, textShadow: "0 2px 8px rgba(0,0,0,.6)" }}>
                      {m ? m.info.map : c.recording}
                    </span>
                    <span className="duration-badge">
                      <IconPlay size={10} />
                    </span>
                  </div>
                </div>
                <div className="card-body">
                  <div className="card-row">
                    <span className="card-meta num">
                      {m ? m.info.mode + " · " : ""}對局 {clipAt(c.at)}
                    </span>
                    <div className="card-actions" style={{ marginLeft: "auto" }} onClick={(e) => e.stopPropagation()}>
                      {confirm === c.path ? (
                        <>
                          <button className="btn sm danger" onClick={() => remove(c)}>
                            確定刪除
                          </button>
                          <button className="btn sm ghost" onClick={() => setConfirm(null)}>
                            取消
                          </button>
                        </>
                      ) : (
                        <>
                          <button className="btn sm ghost" title="在檔案總管顯示" onClick={() => api.reveal(c.path)}>
                            <IconFolder size={15} />
                          </button>
                          <button className="btn sm ghost" title="刪除" onClick={() => setConfirm(c.path)}>
                            <IconTrash size={15} />
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {playing && <ClipModal path={playing.path} title={title(playing)} onClose={() => setPlaying(null)} />}
    </div>
  );
}
