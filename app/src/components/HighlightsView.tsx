// Every highlight range across all matches, newest first. Nothing here is
// a separate file: previews and playback come from the full recordings.
import { useMemo, useState } from "react";
import type { MatchSummary } from "../api";
import { clock, type DateRange, inRange, timeOf } from "../format";
import { type Highlight, highlights, killLabel } from "../highlights";
import { DayGroups, RangeFilter } from "./DayGroups";
import { IconClips, IconPlay } from "./Icons";
import { VideoThumb } from "./VideoThumb";

type Filter = "all" | "multi";

export function HighlightsView({
  matches,
  pre,
  post,
  onOpen,
}: {
  matches: MatchSummary[];
  pre: number;
  post: number;
  onOpen: (h: Highlight) => void;
}) {
  const [filter, setFilter] = useState<Filter>("all");
  const [range, setRange] = useState<DateRange>("all");
  const [hovered, setHovered] = useState<string | null>(null);

  const all = useMemo(
    () => matches.filter((m) => !m.recording).flatMap((m) => highlights(m, pre, post).reverse()),
    [matches, pre, post],
  );
  const shown = all.filter(
    (h) => (filter === "all" || h.kills >= 2) && inRange(h.match.info.started_at, range),
  );
  const multi = all.filter((h) => h.kills >= 2).length;

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1 className="page-title">精華</h1>
          <div className="page-sub">
            {all.length} 個精華區間 · {multi} 個多殺 · 直接從整場錄影播放,不另存檔案
          </div>
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
        </div>
        <RangeFilter value={range} onChange={setRange} />
      </div>

      {shown.length === 0 ? (
        <div className="empty">
          <IconClips size={34} />
          <h3>{all.length === 0 ? "還沒有精華" : "沒有符合的精華"}</h3>
          <div>每次擊殺都會自動成為一個精華區間,相近的擊殺會合併成多殺。</div>
        </div>
      ) : (
        <DayGroups
          items={shown}
          startedAt={(h) => h.match.info.started_at}
          summary={(day) => `${day.length} 個精華 · ${day.filter((h) => h.kills >= 2).length} 個多殺`}
        >
          {(h) => {
            const key = `${h.match.video}@${h.start}`;
            return (
              <div
                key={key}
                className="card"
                onClick={() => onOpen(h)}
                onMouseEnter={() => setHovered(key)}
                onMouseLeave={() => setHovered(null)}
              >
                <div className="thumb">
                  <VideoThumb path={h.match.video} at={h.firstKill} end={h.end} hovered={hovered === key} />
                  <div className="shade" />
                  <div className="badge-tl">
                    <span className={`chip ${h.kills >= 2 ? "gold" : "kill"}`}>{killLabel(h.kills)}</span>
                  </div>
                  <div className="corner">
                    <span style={{ fontWeight: 700, textShadow: "0 2px 8px rgba(0,0,0,.6)" }}>{h.match.info.map}</span>
                    <span className="duration-badge num">
                      <IconPlay size={10} /> {Math.round(h.end - h.start)}s
                    </span>
                  </div>
                </div>
                <div className="card-body">
                  <div className="card-row">
                    <span className="card-meta num">
                      {h.match.info.mode} · 對局 {clock(h.start)} – {clock(h.end)}
                    </span>
                    <span className="card-meta" style={{ marginLeft: "auto" }}>
                      {timeOf(h.match.info.started_at)}
                    </span>
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
