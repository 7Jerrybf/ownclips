import { useState } from "react";
import type { MatchSummary } from "../api";
import { api } from "../api";
import { clock, count, type DateRange, inRange, size, timeOf } from "../format";
import { DayGroups, RangeFilter } from "./DayGroups";
import { IconFolder, IconKeep, IconMatches, IconRefresh, IconTrash } from "./Icons";
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
  const shown = matches.filter((m) => inRange(m.info.started_at, range));
  const totalKills = matches.reduce((n, m) => n + count(m.info.events, "kill"), 0);

  async function toggleKeep(m: MatchSummary) {
    try {
      await api.setKeep(m.video, !m.info.keep);
    } catch (e) {
      notify(String(e), "error");
    }
  }

  async function remove(m: MatchSummary) {
    try {
      await api.deleteMatch(m.video);
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
        </div>
      )}

      {shown.length === 0 ? (
        <div className="empty">
          <IconMatches size={34} />
          <h3>{matches.length === 0 ? "還沒有錄影" : "這段期間沒有錄影"}</h3>
          <div>開著 OwnClips 去打一場,對局開始時會自動錄影,結束後出現在這裡。</div>
        </div>
      ) : (
        <DayGroups items={shown} startedAt={(m) => m.info.started_at} summary={daySummary}>
          {(m) => {
            const kills = count(m.info.events, "kill");
            const deaths = count(m.info.events, "death");
            const assists = count(m.info.events, "assist");
            const dur = m.info.duration ?? 0;
            return (
              <div
                key={m.video}
                className="card"
                onClick={() => !m.recording && onOpen(m)}
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
                    <span className="chip">{m.info.mode}</span>
                    {m.info.keep && <span className="chip keep">已保留</span>}
                  </div>
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
