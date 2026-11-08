// The clips exported from matches (the files in the recordings' `clips`
// folder), newest first.
import { useState } from "react";
import type { ClipSummary } from "../api";
import { api } from "../api";
import { clock, type DateRange, inRange, size, stampOf, timeOf } from "../format";
import { DayGroups, RangeFilter } from "./DayGroups";
import { IconClips, IconFolder, IconTrash } from "./Icons";
import { VideoThumb } from "./VideoThumb";

/** What a clip shows as its title: the map it came from, or its file name. */
export const clipTitle = (c: ClipSummary) => c.info?.map ?? c.name;

/** The folder a file is in. */
const folderOf = (path: string) => path.replace(/[\\/][^\\/]+$/, "");

export function HighlightsView({
  clips,
  onOpen,
  notify,
}: {
  clips: ClipSummary[];
  onOpen: (c: ClipSummary) => void;
  notify: (message: string, kind?: "error" | "success") => void;
}) {
  const [range, setRange] = useState<DateRange>("all");
  const [hovered, setHovered] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);

  const shown = clips.filter((c) => inRange(stampOf(c.modified_ms), range));
  const total = clips.reduce((n, c) => n + c.size, 0);

  async function remove(c: ClipSummary) {
    setConfirm(null);
    try {
      await api.deleteClips([c.path]);
      notify("已移到資源回收筒", "success");
    } catch (e) {
      notify(String(e), "error");
    }
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1 className="page-title">精華</h1>
          <div className="page-sub">
            {clips.length} 個剪輯片段{clips.length > 0 && ` · ${size(total)}`} · 在對局裡設好起點與終點後按剪刀匯出
          </div>
        </div>
        {clips.length > 0 && (
          <div className="page-actions">
            <button className="btn" onClick={() => api.openPath(folderOf(clips[0].path))}>
              <IconFolder size={16} /> 開啟資料夾
            </button>
          </div>
        )}
      </div>

      {clips.length > 0 && (
        <div className="filter-row">
          <RangeFilter value={range} onChange={setRange} />
        </div>
      )}

      {shown.length === 0 ? (
        <div className="empty">
          <IconClips size={34} />
          <h3>{clips.length === 0 ? "還沒有剪輯片段" : "這段期間沒有剪輯片段"}</h3>
          <div>打開一場對局,在時間軸上拖曳金色把手設定起點與終點,再按剪刀匯出,片段就會出現在這裡。</div>
        </div>
      ) : (
        <DayGroups items={shown} startedAt={(c) => stampOf(c.modified_ms)} summary={(day) => `${day.length} 個片段`}>
          {(c) => (
            <div
              key={c.path}
              className="card"
              onClick={() => onOpen(c)}
              onMouseEnter={() => setHovered(c.path)}
              onMouseLeave={() => setHovered(null)}
            >
              <div className="thumb">
                <VideoThumb path={c.path} at={0.5} hovered={hovered === c.path} />
                <div className="shade" />
                <div className="corner">
                  <span style={{ fontWeight: 700, textShadow: "0 2px 8px rgba(0,0,0,.6)" }}>{clipTitle(c)}</span>
                </div>
              </div>
              <div className="card-body">
                <div className="card-row">
                  <span className="card-meta num">
                    {[c.info?.mode, c.at !== null ? `對局 ${clock(c.at)}` : null].filter(Boolean).join(" · ") ||
                      "剪輯片段"}
                  </span>
                  <span className="card-meta" style={{ marginLeft: "auto" }}>
                    {timeOf(stampOf(c.modified_ms))}
                  </span>
                </div>
                <div className="card-row" onClick={(e) => e.stopPropagation()}>
                  <span className="card-meta">{size(c.size)}</span>
                  <div className="card-actions" style={{ marginLeft: "auto" }}>
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
                        <button className="btn sm ghost" title="刪除這個片段" onClick={() => setConfirm(c.path)}>
                          <IconTrash size={15} />
                        </button>
                      </>
                    )}
                  </div>
                </div>
              </div>
            </div>
          )}
        </DayGroups>
      )}
    </div>
  );
}
