import { useState } from "react";
import type { MatchSummary } from "../api";
import { api } from "../api";
import { clock, count, type DateRange, gameLabel, inRange, size, timeOf } from "../format";
import { DayGroups, RangeFilter } from "./DayGroups";
import { IconCheck, IconFolder, IconKeep, IconMatches, IconRefresh, IconTrash } from "./Icons";
import { VideoThumb } from "./VideoThumb";

export function MatchesView({
  matches,
  onOpen,
  onChanged,
  notify,
}: {
  matches: MatchSummary[];
  onOpen: (m: MatchSummary) => void;
  onChanged: () => void;
  notify: (message: string, kind?: "error" | "success") => void;
}) {
  const [confirm, setConfirm] = useState<string | null>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [range, setRange] = useState<DateRange>("all");
  // Selection mode for deleting several matches at once.
  const [selecting, setSelecting] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [confirmBulk, setConfirmBulk] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const shown = matches.filter((m) => inRange(m.info.started_at, range));
  const totalKills = matches.reduce((n, m) => n + count(m.info.events, "kill"), 0);
  // Only what's on screen counts: switching the date filter drops the rest.
  const selected = shown.filter((m) => picked.has(m.video));
  const selectedKept = selected.filter((m) => m.info.keep).length;

  function setPick(videos: string[], on: boolean) {
    setConfirmBulk(false);
    setPicked((prev) => {
      const next = new Set(prev);
      videos.forEach((v) => (on ? next.add(v) : next.delete(v)));
      return next;
    });
  }

  function selectState(ms: MatchSummary[]): CheckState {
    const ids = selectable(ms);
    const n = ids.filter((v) => picked.has(v)).length;
    return n === 0 ? "none" : n === ids.length ? "all" : "some";
  }

  function stopSelecting() {
    setSelecting(false);
    setPicked(new Set());
    setConfirmBulk(false);
  }

  async function removeSelected() {
    setDeleting(true);
    try {
      const n = await api.deleteMatches(selected.map((m) => m.video));
      notify(`已將 ${n} 場錄影移到資源回收筒`, "success");
      stopSelecting();
      onChanged();
    } catch (e) {
      notify(String(e), "error");
    }
    setDeleting(false);
    setConfirmBulk(false);
  }

  async function toggleKeep(m: MatchSummary) {
    try {
      await api.setKeep(m.video, !m.info.keep);
    } catch (e) {
      notify(String(e), "error");
    }
  }

  async function remove(m: MatchSummary) {
    try {
      await api.deleteMatches([m.video]);
      notify("已移到資源回收筒", "success");
      onChanged();
    } catch (e) {
      notify(String(e), "error");
    }
    setConfirm(null);
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1 className="page-title">對局</h1>
          <div className="page-sub">
            {matches.length} 場錄影 · 共 {totalKills} 次擊殺
          </div>
        </div>
        <div className="page-actions">
          {matches.length > 0 && (
            <button
              className={`btn ${selecting ? "primary" : "ghost"}`}
              onClick={() => (selecting ? stopSelecting() : setSelecting(true))}
            >
              <IconCheck size={16} /> {selecting ? "完成" : "選取"}
            </button>
          )}
          <button className="btn ghost" onClick={onChanged}>
            <IconRefresh size={16} /> 重新整理
          </button>
          <button className="btn" onClick={() => api.recordingsDir().then(api.openPath)}>
            <IconFolder size={16} /> 錄影資料夾
          </button>
        </div>
      </div>

      {matches.length > 0 && (
        <div className="filter-row">
          <RangeFilter value={range} onChange={setRange} />
          {selecting && (
            <div className="select-bar">
              <Check
                state={selectState(shown)}
                onClick={() => setPick(selectable(shown), selectState(shown) !== "all")}
                label="全選"
              />
              <span className="card-meta num">
                已選 {selected.length} 場{selected.length > 0 && ` · ${size(selected.reduce((n, m) => n + m.size, 0))}`}
              </span>
              {confirmBulk ? (
                <>
                  <span className="warn">
                    確定刪除 {selected.length} 場?{selectedKept > 0 && `其中 ${selectedKept} 場是已保留的。`}
                  </span>
                  <button className="btn sm danger" disabled={deleting} onClick={removeSelected}>
                    {deleting ? "刪除中…" : "確定刪除"}
                  </button>
                  <button className="btn sm ghost" disabled={deleting} onClick={() => setConfirmBulk(false)}>
                    取消
                  </button>
                </>
              ) : (
                <button className="btn sm danger" disabled={selected.length === 0} onClick={() => setConfirmBulk(true)}>
                  <IconTrash size={15} /> 刪除
                </button>
              )}
            </div>
          )}
        </div>
      )}

      {shown.length === 0 ? (
        <div className="empty">
          <IconMatches size={34} />
          <h3>{matches.length === 0 ? "還沒有錄影" : "這段期間沒有錄影"}</h3>
          <div>開著 OwnClips 去打一場,對局開始時會自動錄影,結束後出現在這裡。</div>
        </div>
      ) : (
        <DayGroups
          items={shown}
          startedAt={(m) => m.info.started_at}
          summary={daySummary}
          headStart={
            selecting
              ? (day) => (
                  <Check
                    state={selectState(day)}
                    onClick={() => setPick(selectable(day), selectState(day) !== "all")}
                    title="選取這天全部"
                  />
                )
              : undefined
          }
        >
          {(m) => {
            const kills = count(m.info.events, "kill");
            const deaths = count(m.info.events, "death");
            const assists = count(m.info.events, "assist");
            const dur = m.info.duration ?? 0;
            const isPicked = picked.has(m.video);
            return (
              <div
                key={m.video}
                className={`card ${isPicked ? "picked" : ""}`}
                onClick={() => {
                  if (m.recording) return;
                  if (selecting) setPick([m.video], !isPicked);
                  else onOpen(m);
                }}
                onMouseEnter={() => setHovered(m.video)}
                onMouseLeave={() => setHovered(null)}
              >
                <div className="thumb">
                  <VideoThumb
                    path={m.video}
                    at={thumbTime(m)}
                    end={thumbTime(m) + 6}
                    hovered={hovered === m.video}
                    disabled={m.recording}
                  />
                  <div className="shade" />
                  <div className="badge-tl">
                    {m.recording && <span className="chip rec">● 錄影 / 處理中</span>}
                    {m.info.game === "league" && <span className="chip">{gameLabel.league}</span>}
                    <span className="chip">{m.info.mode}</span>
                    {m.info.keep && <span className="chip keep">已保留</span>}
                  </div>
                  {selecting && !m.recording && (
                    <div className="badge-tr">
                      <Check state={isPicked ? "all" : "none"} />
                    </div>
                  )}
                  <div className="corner">
                    <span className="map-name">{m.info.map}</span>
                    {dur > 0 && <span className="duration-badge num">{clock(dur)}</span>}
                  </div>
                </div>
                <div className="card-body">
                  <div className="card-row">
                    <span className="chip kill num">{kills} 擊殺</span>
                    <span className="chip death num">{deaths} 死亡</span>
                    {assists > 0 && <span className="chip assist num">{assists} 助攻</span>}
                    <span className="card-meta" style={{ marginLeft: "auto" }}>
                      {timeOf(m.info.started_at)}
                    </span>
                  </div>
                  {dur > 0 && (
                    <div className="mini-timeline">
                      {m.info.events.map((e, i) => (
                        <span
                          key={i}
                          style={{
                            left: `${(e.t / dur) * 100}%`,
                            background: `var(--${e.kind})`,
                          }}
                        />
                      ))}
                    </div>
                  )}
                  <div className="card-row" onClick={(e) => e.stopPropagation()}>
                    <span className="card-meta">{size(m.size)}</span>
                    <div className="card-actions" style={{ marginLeft: "auto" }}>
                      {confirm === m.video ? (
                        <>
                          <button className="btn sm danger" onClick={() => remove(m)}>
                            確定刪除
                          </button>
                          <button className="btn sm ghost" onClick={() => setConfirm(null)}>
                            取消
                          </button>
                        </>
                      ) : (
                        <>
                          <button
                            className={`btn sm ghost ${m.info.keep ? "keep-on" : ""}`}
                            title={m.info.keep ? "取消保留" : "保留這場:超過容量上限時不會被自動刪除"}
                            disabled={m.recording}
                            onClick={() => toggleKeep(m)}
                          >
                            <IconKeep size={15} filled={m.info.keep} />
                          </button>
                          <button className="btn sm ghost" title="在檔案總管顯示" onClick={() => api.reveal(m.video)}>
                            <IconFolder size={15} />
                          </button>
                          <button
                            className="btn sm ghost"
                            title="刪除這場錄影"
                            disabled={m.recording}
                            onClick={() => setConfirm(m.video)}
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
    </div>
  );
}

/** Matches that can be deleted (not the one being recorded). */
function selectable(ms: MatchSummary[]): string[] {
  return ms.filter((m) => !m.recording).map((m) => m.video);
}

type CheckState = "all" | "some" | "none";

function Check({
  state,
  onClick,
  label,
  title,
}: {
  state: CheckState;
  onClick?: () => void;
  label?: string;
  title?: string;
}) {
  return (
    <button
      type="button"
      className={`check ${state}`}
      role="checkbox"
      aria-checked={state === "all" ? true : state === "some" ? "mixed" : false}
      title={title}
      // Inside a card the card's own click toggles it.
      tabIndex={onClick ? 0 : -1}
      onClick={
        onClick &&
        ((e) => {
          e.stopPropagation();
          onClick();
        })
      }
    >
      <span className="box">{state === "all" ? <IconCheck size={12} /> : state === "some" ? "–" : null}</span>
      {label}
    </button>
  );
}

function daySummary(day: MatchSummary[]) {
  const total = (kind: "kill" | "death" | "assist") => day.reduce((n, m) => n + count(m.info.events, kind), 0);
  return `${day.length} 場 · ${total("kill")} / ${total("death")} / ${total("assist")}`;
}

/** A representative frame: the middle kill, or a third of the way in. */
function thumbTime(m: MatchSummary): number {
  const kills = m.info.events.filter((e) => e.kind === "kill").sort((a, b) => a.t - b.t);
  if (kills.length > 0) return kills[Math.floor(kills.length / 2)].t;
  const dur = m.info.duration ?? 0;
  return dur > 60 ? dur * 0.35 : 3;
}
