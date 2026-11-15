import { useState } from "react";
import type { MatchSummary } from "../api";
import { api } from "../api";
import { clock, count, type DateRange, gameLabel, hasKda, inRange, size, timeOf } from "../format";
import { DayGroups, RangeFilter } from "./DayGroups";
import { IconAssist, IconDeath, IconFolder, IconKeep, IconKill, IconMatches, IconRefresh, IconTrash } from "./Icons";
import { Check, SelectBar, useSelection } from "./Selection";
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
  const sel = useSelection();
  const [confirmBulk, setConfirmBulk] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const shown = matches.filter((m) => inRange(m.info.started_at, range));
  const totalKills = matches.filter((m) => hasKda(m.info.game)).reduce((n, m) => n + count(m.info.events, "kill"), 0);
  const anyKda = matches.some((m) => hasKda(m.info.game));
  // Only what's on screen counts: switching the date filter drops the rest.
  const selected = shown.filter((m) => sel.picked.has(m.video) && !m.recording);
  const selectedKept = selected.filter((m) => m.info.keep).length;
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

  async function removeSelected() {
    setDeleting(true);
    try {
      const n = await api.deleteMatches(selected.map((m) => m.video));
      notify(`已移到回收筒 · ${n} 段`, "success");
      clear();
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
      notify("已移到回收筒", "success");
      onChanged();
    } catch (e) {
      notify(String(e), "error");
    }
    setConfirm(null);
  }

  // With a selection, any card's trash deletes the whole selection (plus
  // that card).
  function askDelete(m: MatchSummary) {
    if (selecting) {
      sel.set([m.video], true);
      setConfirmBulk(true);
    } else {
      setConfirm(m.video);
    }
  }

  return (
    <div className={`page ${selecting ? "selecting" : ""}`}>
      <div className="page-head">
        <div>
          <h1 className="page-title">錄影</h1>
          <div className="page-sub num">
            {matches.length} 段{anyKda && ` · ${totalKills} 擊殺`}
          </div>
        </div>
        <div className="page-actions">
          <button className="btn ghost icon-btn" title="重新整理" onClick={onChanged}>
            <IconRefresh size={16} />
          </button>
          <button className="btn ghost icon-btn" title="錄影資料夾" onClick={() => api.recordingsDir().then(api.openPath)}>
            <IconFolder size={16} />
          </button>
        </div>
      </div>

      {matches.length > 0 && (
        <div className="filter-row">
          <RangeFilter value={range} onChange={setRange} />
        </div>
      )}

      {shown.length === 0 ? (
        <div className="empty">
          <IconMatches size={34} />
          <h3>{matches.length === 0 ? "尚無錄影" : "無錄影"}</h3>
          {matches.length === 0 && <div>開打自動錄影</div>}
        </div>
      ) : (
        <DayGroups
          items={shown}
          startedAt={(m) => m.info.started_at}
          summary={daySummary}
          headStart={(day) => (
            <Check state={sel.state(selectable(day))} onClick={() => pick(selectable(day))} title="選取整天" />
          )}
        >
          {(m) => {
            const kills = count(m.info.events, "kill");
            const deaths = count(m.info.events, "death");
            const assists = count(m.info.events, "assist");
            const kda = hasKda(m.info.game);
            const dur = m.info.duration ?? 0;
            const isPicked = sel.picked.has(m.video);
            return (
              <div
                key={m.video}
                className={`card ${isPicked ? "picked" : ""}`}
                onClick={() => {
                  if (m.recording) return;
                  if (selecting) pick([m.video], !isPicked);
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
                    {m.recording && <span className="chip rec">● REC</span>}
                    {m.info.game === "league" && <span className="chip">{gameLabel.league}</span>}
                    <span className="chip">{m.info.mode}</span>
                    {m.info.keep && (
                      <span className="chip keep" title="已保留">
                        <IconKeep size={12} filled />
                      </span>
                    )}
                  </div>
                  {!m.recording && (
                    <div className="badge-tr card-check">
                      <Check state={isPicked ? "all" : "none"} onClick={() => pick([m.video], !isPicked)} />
                    </div>
                  )}
                  <div className="corner">
                    <span className="map-name">{m.info.map}</span>
                    {dur > 0 && <span className="duration-badge num">{clock(dur)}</span>}
                  </div>
                </div>
                <div className="card-body">
                  <div className="card-row">
                    {kda && (
                      <>
                        <span className="chip kill num" title="擊殺">
                          <IconKill size={12} /> {kills}
                        </span>
                        <span className="chip death num" title="死亡">
                          <IconDeath size={12} /> {deaths}
                        </span>
                        {assists > 0 && (
                          <span className="chip assist num" title="助攻">
                            <IconAssist size={12} /> {assists}
                          </span>
                        )}
                      </>
                    )}
                    <span className="card-meta" style={{ marginLeft: "auto" }}>
                      {timeOf(m.info.started_at)}
                    </span>
                  </div>
                  {kda && dur > 0 && (
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
                      {confirm === m.video && !selecting ? (
                        <>
                          <button className="btn sm danger" onClick={() => remove(m)}>
                            刪除
                          </button>
                          <button className="btn sm ghost" onClick={() => setConfirm(null)}>
                            取消
                          </button>
                        </>
                      ) : (
                        <>
                          <button
                            className={`btn sm ghost ${m.info.keep ? "keep-on" : ""}`}
                            title={m.info.keep ? "取消保留" : "保留(不自動清理)"}
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
                            title={selecting ? `刪除已選` : "刪除"}
                            disabled={m.recording}
                            onClick={() => askDelete(m)}
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
        detail={size(selected.reduce((n, m) => n + m.size, 0))}
        allState={sel.state(selectable(shown))}
        onAll={() => pick(selectable(shown))}
        confirming={confirmBulk}
        busy={deleting}
        warning={selectedKept > 0 ? `含 ${selectedKept} 段已保留` : null}
        onDelete={() => setConfirmBulk(true)}
        onConfirm={removeSelected}
        onCancel={() => setConfirmBulk(false)}
        onClear={clear}
      />
    </div>
  );
}

/** Matches that can be deleted (not the one being recorded). */
function selectable(ms: MatchSummary[]): string[] {
  return ms.filter((m) => !m.recording).map((m) => m.video);
}

function daySummary(day: MatchSummary[]) {
  const kdaMatches = day.filter((m) => hasKda(m.info.game));
  const total = (kind: "kill" | "death" | "assist") => kdaMatches.reduce((n, m) => n + count(m.info.events, kind), 0);
  if (kdaMatches.length === 0) return `${day.length} 段`;
  return `${day.length} 段 · ${total("kill")} / ${total("death")} / ${total("assist")}`;
}

/** A representative frame: the middle kill, or a third of the way in. */
function thumbTime(m: MatchSummary): number {
  const kills = m.info.events.filter((e) => e.kind === "kill").sort((a, b) => a.t - b.t);
  if (kills.length > 0) return kills[Math.floor(kills.length / 2)].t;
  const dur = m.info.duration ?? 0;
  return dur > 60 ? dur * 0.35 : 3;
}
