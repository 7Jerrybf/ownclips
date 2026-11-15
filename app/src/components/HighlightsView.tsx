// The clips exported from matches (the files in the recordings' `clips`
// folder), newest first.
import { useState } from "react";
import type { ClipSummary } from "../api";
import { api } from "../api";
import { clock, type DateRange, inRange, size, stampOf, timeOf } from "../format";
import { DayGroups, RangeFilter } from "./DayGroups";
import { IconClips, IconFolder, IconTrash } from "./Icons";
import { Check, SelectBar, useSelection } from "./Selection";
import { VideoThumb } from "./VideoThumb";

/** What a clip shows as its title: the map it came from, or its file name. */
export const clipTitle = (c: ClipSummary) => c.info?.map ?? c.name;

/** The folder a file is in. */
const folderOf = (path: string) => path.replace(/[\\/][^\\/]+$/, "");

const paths = (cs: ClipSummary[]) => cs.map((c) => c.path);

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
  const sel = useSelection();
  const [confirmBulk, setConfirmBulk] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const shown = clips.filter((c) => inRange(stampOf(c.modified_ms), range));
  const total = clips.reduce((n, c) => n + c.size, 0);
  const selected = shown.filter((c) => sel.picked.has(c.path));
  const selecting = selected.length > 0;

  function pick(ids: string[], on?: boolean) {
    setConfirmBulk(false);
    if (on === undefined) sel.toggle(ids);
    else sel.set(ids, on);
  }

  function clear() {
    sel.clear();
    setConfirmBulk(false);
  }

  async function remove(c: ClipSummary) {
    setConfirm(null);
    try {
      await api.deleteClips([c.path]);
      notify("已移到回收筒", "success");
    } catch (e) {
      notify(String(e), "error");
    }
  }

  async function removeSelected() {
    setDeleting(true);
    try {
      const n = await api.deleteClips(paths(selected));
      notify(`已移到回收筒 · ${n} 個`, "success");
      clear();
    } catch (e) {
      notify(String(e), "error");
    }
    setDeleting(false);
    setConfirmBulk(false);
  }

  function askDelete(c: ClipSummary) {
    if (selecting) {
      sel.set([c.path], true);
      setConfirmBulk(true);
    } else {
      setConfirm(c.path);
    }
  }

  return (
    <div className={`page ${selecting ? "selecting" : ""}`}>
      <div className="page-head">
        <div>
          <h1 className="page-title">精華</h1>
          <div className="page-sub num">
            {clips.length} 個{clips.length > 0 && ` · ${size(total)}`}
          </div>
        </div>
        {clips.length > 0 && (
          <div className="page-actions">
            <button className="btn ghost icon-btn" title="開啟資料夾" onClick={() => api.openPath(folderOf(clips[0].path))}>
              <IconFolder size={16} />
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
          <h3>{clips.length === 0 ? "尚無片段" : "無片段"}</h3>
          {clips.length === 0 && <div>對局 → 設定起點、終點 → ✂ 匯出</div>}
        </div>
      ) : (
        <DayGroups
          items={shown}
          startedAt={(c) => stampOf(c.modified_ms)}
          summary={(day) => `${day.length} 個`}
          headStart={(day) => <Check state={sel.state(paths(day))} onClick={() => pick(paths(day))} title="選取整天" />}
        >
          {(c) => {
            const isPicked = sel.picked.has(c.path);
            return (
              <div
                key={c.path}
                className={`card ${isPicked ? "picked" : ""}`}
                onClick={() => (selecting ? pick([c.path], !isPicked) : onOpen(c))}
                onMouseEnter={() => setHovered(c.path)}
                onMouseLeave={() => setHovered(null)}
              >
                <div className="thumb">
                  <VideoThumb path={c.path} at={0.5} hovered={hovered === c.path} />
                  <div className="shade" />
                  <div className="badge-tr card-check">
                    <Check state={isPicked ? "all" : "none"} onClick={() => pick([c.path], !isPicked)} />
                  </div>
                  <div className="corner">
                    <span style={{ fontWeight: 700, textShadow: "0 2px 8px rgba(0,0,0,.6)" }}>{clipTitle(c)}</span>
                  </div>
                </div>
                <div className="card-body">
                  <div className="card-row">
                    <span className="card-meta num">
                      {[c.info?.mode, c.at !== null ? clock(c.at) : null].filter(Boolean).join(" · ")}
                    </span>
                    <span className="card-meta" style={{ marginLeft: "auto" }}>
                      {timeOf(stampOf(c.modified_ms))}
                    </span>
                  </div>
                  <div className="card-row" onClick={(e) => e.stopPropagation()}>
                    <span className="card-meta">{size(c.size)}</span>
                    <div className="card-actions" style={{ marginLeft: "auto" }}>
                      {confirm === c.path && !selecting ? (
                        <>
                          <button className="btn sm danger" onClick={() => remove(c)}>
                            刪除
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
                          <button
                            className="btn sm ghost"
                            title={selecting ? "刪除已選" : "刪除"}
                            onClick={() => askDelete(c)}
                          >
                            <IconTrash size={15} />
                          </button>
                        </>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            );
          }}
        </DayGroups>
      )}

      <SelectBar
        count={selected.length}
        detail={size(selected.reduce((n, c) => n + c.size, 0))}
        allState={sel.state(paths(shown))}
        onAll={() => pick(paths(shown))}
        confirming={confirmBulk}
        busy={deleting}
        onDelete={() => setConfirmBulk(true)}
        onConfirm={removeSelected}
        onCancel={() => setConfirmBulk(false)}
        onClear={clear}
      />
    </div>
  );
}
