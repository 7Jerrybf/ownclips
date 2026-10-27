import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type { MatchSummary, RecorderStatus, Toast } from "./api";
import { api } from "./api";
import { HighlightsView } from "./components/HighlightsView";
import { IconClips, IconMatches, IconPlay, IconSettings } from "./components/Icons";
import { MatchDetail } from "./components/MatchDetail";
import { MatchesView } from "./components/MatchesView";
import { SettingsView } from "./components/SettingsView";
import { Toasts, type ToastItem } from "./components/Toasts";
import { clock } from "./format";

type Page = "matches" | "highlights" | "settings";

export default function App() {
  const [page, setPage] = useState<Page>("matches");
  const [matches, setMatches] = useState<MatchSummary[]>([]);
  const [open, setOpen] = useState<{ match: MatchSummary; range?: [number, number] } | null>(null);
  const [clipSecs, setClipSecs] = useState<[number, number]>([8, 3]);
  const [status, setStatus] = useState<RecorderStatus | null>(null);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [calibrated, setCalibrated] = useState(true);
  const nextId = useRef(1);

  const notify = useCallback((message: string, kind: Toast["kind"] = "info") => {
    const id = nextId.current++;
    setToasts((t) => [...t, { id, kind, message }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4500);
  }, []);

  const refresh = useCallback(() => {
    api.listMatches().then(setMatches).catch((e) => notify(String(e), "error"));
    api.getConfig().then((c) => {
      setCalibrated(!!c.killfeed_region);
      setClipSecs([c.clip_pre_seconds, c.clip_post_seconds]);
    });
  }, [notify]);

  useEffect(() => {
    refresh();
    api.status().then(setStatus);
    const unlisten = [
      listen<RecorderStatus>("recorder-status", (e) => {
        setStatus(e.payload);
        if (e.payload.state === "recording") refresh();
      }),
      listen<Toast>("toast", (e) => notify(e.payload.message, e.payload.kind)),
      listen("library-changed", () => refresh()),
    ];
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, [refresh, notify]);

  // Keep the open match in sync after a refresh.
  useEffect(() => {
    if (!open) return;
    const m = matches.find((x) => x.video === open.match.video);
    setOpen(m ? { ...open, match: m } : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [matches]);

  const go = (p: Page) => {
    setPage(p);
    setOpen(null);
  };

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <IconPlay size={14} />
          </div>
          OwnClips
        </div>
        <button className={`nav-item ${page === "matches" ? "active" : ""}`} onClick={() => go("matches")}>
          <IconMatches /> 對局 <span className="count num">{matches.length}</span>
        </button>
        <button className={`nav-item ${page === "highlights" ? "active" : ""}`} onClick={() => go("highlights")}>
          <IconClips /> 精華
        </button>
        <button className={`nav-item ${page === "settings" ? "active" : ""}`} onClick={() => go("settings")}>
          <IconSettings /> 設定
        </button>
        <div className="sidebar-spacer" />
        <StatusCard status={status} calibrated={calibrated} onFix={() => go("settings")} />
      </aside>

      <main className="main">
        {open ? (
          <MatchDetail
            key={`${open.match.video}@${open.range?.[0] ?? ""}`}
            match={open.match}
            pre={clipSecs[0]}
            post={clipSecs[1]}
            initialRange={open.range}
            onBack={() => setOpen(null)}
            notify={notify}
          />
        ) : page === "matches" ? (
          <MatchesView matches={matches} onOpen={(m) => setOpen({ match: m })} onChanged={refresh} notify={notify} />
        ) : page === "highlights" ? (
          <HighlightsView
            matches={matches}
            pre={clipSecs[0]}
            post={clipSecs[1]}
            onOpen={(h) => setOpen({ match: h.match, range: [h.start, h.end] })}
          />
        ) : null}
        {!open && page === "settings" && <SettingsView notify={notify} onSaved={refresh} />}
      </main>
      <Toasts items={toasts} />
    </div>
  );
}

function StatusCard({
  status,
  calibrated,
  onFix,
}: {
  status: RecorderStatus | null;
  calibrated: boolean;
  onFix: () => void;
}) {
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, []);

  if (status?.state === "recording") {
    const elapsed = status.started_at ? (Date.now() - status.started_at) / 1000 : 0;
    return (
      <div className="status-card recording">
        <div className="status-head">
          <span className="dot live" /> 錄影中 <span className="num" style={{ marginLeft: "auto" }}>{clock(elapsed)}</span>
        </div>
        <div className="status-sub">
          {status.mode} · {status.map}
        </div>
        <div className="status-kd num">
          <span style={{ color: "var(--kill)" }}>{status.kills} 擊殺</span>
          <span style={{ color: "var(--death)" }}>{status.deaths} 死亡</span>
          <span style={{ color: "var(--assist)" }}>{status.assists} 助攻</span>
        </div>
      </div>
    );
  }
  return (
    <div className="status-card">
      <div className="status-head">
        <span className={`dot ${calibrated ? "ok" : ""}`} /> {calibrated ? "待命中" : "需要設定"}
      </div>
      <div className="status-sub">
        {calibrated ? "Valorant / 英雄聯盟對局開始時會自動錄影" : "還沒校準 killfeed,Valorant 錄影時不會偵測擊殺"}
      </div>
      {!calibrated && (
        <button className="btn sm" onClick={onFix}>
          前往校準
        </button>
      )}
    </div>
  );
}
