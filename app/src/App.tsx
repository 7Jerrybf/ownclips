import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type { ClipSummary, MatchSummary, RecorderStatus, Toast } from "./api";
import { api } from "./api";
import { ClipDetail } from "./components/ClipDetail";
import { HighlightsView } from "./components/HighlightsView";
import { IconClips, IconMatches, IconPlay, IconSettings } from "./components/Icons";
import { MatchDetail } from "./components/MatchDetail";
import { MatchesView } from "./components/MatchesView";
import { SettingsView } from "./components/SettingsView";
import { Toasts, type ToastItem } from "./components/Toasts";
import { UpdateCard, useUpdates } from "./components/Update";
import { WindowPicker } from "./components/WindowPicker";
import { clock, hotkeyLabel } from "./format";

type Page = "matches" | "highlights" | "settings";

export default function App() {
  const [page, setPage] = useState<Page>("matches");
  const [matches, setMatches] = useState<MatchSummary[]>([]);
  const [open, setOpen] = useState<{ match: MatchSummary; range?: [number, number] } | null>(null);
  const [clips, setClips] = useState<ClipSummary[]>([]);
  const [openClip, setOpenClip] = useState<ClipSummary | null>(null);
  const [clipSecs, setClipSecs] = useState<[number, number]>([8, 3]);
  const [status, setStatus] = useState<RecorderStatus | null>(null);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [calibrated, setCalibrated] = useState(true);
  const [picking, setPicking] = useState(false);
  const [hotkey, setHotkey] = useState<string | null>(null);
  const nextId = useRef(1);

  const notify = useCallback((message: string, kind: Toast["kind"] = "info") => {
    const id = nextId.current++;
    setToasts((t) => [...t, { id, kind, message }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4500);
  }, []);
  const updates = useUpdates(notify);

  const refreshClips = useCallback(() => {
    api.listClips().then(setClips).catch((e) => notify(String(e), "error"));
  }, [notify]);

  const refresh = useCallback(() => {
    api.listMatches().then(setMatches).catch((e) => notify(String(e), "error"));
    refreshClips();
    api.getConfig().then((c) => {
      setCalibrated(!!c.killfeed_region);
      setHotkey(c.manual_hotkey);
      setClipSecs([c.clip_pre_seconds, c.clip_post_seconds]);
    });
  }, [notify, refreshClips]);

  useEffect(() => {
    refresh();
    api.status().then(setStatus);
    api.hotkeyError().then((e) => e && notify(e, "error"));
    const unlisten = [
      listen<RecorderStatus>("recorder-status", (e) => {
        setStatus(e.payload);
        if (e.payload.state === "recording") refresh();
      }),
      listen<Toast>("toast", (e) => notify(e.payload.message, e.payload.kind)),
      listen("library-changed", () => refresh()),
      listen("clips-changed", () => refreshClips()),
    ];
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, [refresh, refreshClips, notify]);

  // Drop the open clip once it's deleted.
  useEffect(() => {
    setOpenClip((c) => (c && clips.some((x) => x.path === c.path) ? c : null));
  }, [clips]);

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
    setOpenClip(null);
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
          <IconClips /> 精華 <span className="count num">{clips.length}</span>
        </button>
        <button className={`nav-item ${page === "settings" ? "active" : ""}`} onClick={() => go("settings")}>
          <IconSettings /> 設定
        </button>
        <div className="sidebar-spacer" />
        <UpdateCard updates={updates} recording={status?.state === "recording"} />
        <StatusCard
          status={status}
          calibrated={calibrated}
          hotkey={hotkey}
          onFix={() => go("settings")}
          onRecord={() => setPicking(true)}
          notify={notify}
        />
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
        ) : page === "highlights" && openClip ? (
          <ClipDetail
            key={openClip.path}
            clip={openClip}
            onBack={() => setOpenClip(null)}
            onOpenMatch={(() => {
              const match = matches.find((m) => m.video === openClip.source);
              return match && ((range: [number, number]) => setOpen({ match, range }));
            })()}
            notify={notify}
          />
        ) : page === "highlights" ? (
          <HighlightsView clips={clips} onOpen={setOpenClip} notify={notify} />
        ) : null}
        {!open && page === "settings" && <SettingsView notify={notify} onSaved={refresh} updates={updates} />}
      </main>
      {picking && (
        <WindowPicker
          notify={notify}
          onClose={() => setPicking(false)}
          onPick={(w) => {
            setPicking(false);
            api.startManual(w.hwnd).catch((e) => notify(String(e), "error"));
          }}
        />
      )}
      <Toasts items={toasts} />
    </div>
  );
}

function StatusCard({
  status,
  calibrated,
  hotkey,
  onFix,
  onRecord,
  notify,
}: {
  status: RecorderStatus | null;
  calibrated: boolean;
  hotkey: string | null;
  onFix: () => void;
  onRecord: () => void;
  notify: (message: string, kind?: Toast["kind"]) => void;
}) {
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, []);

  // Stopping takes a moment; it's over once the status moves on.
  const [stopping, setStopping] = useState(false);
  useEffect(() => setStopping(false), [status?.video]);

  if (status?.state === "recording") {
    const elapsed = status.started_at ? (Date.now() - status.started_at) / 1000 : 0;
    if (status.manual) {
      return (
        <div className="status-card recording">
          <div className="status-head">
            <span className="dot live" /> 手動錄影中 <span className="num" style={{ marginLeft: "auto" }}>{clock(elapsed)}</span>
          </div>
          <div className="status-sub">{status.map}</div>
          <button
            className="btn sm danger"
            disabled={stopping}
            onClick={() => {
              setStopping(true);
              api.stopManual().catch((e) => {
                setStopping(false);
                notify(String(e), "error");
              });
            }}
          >
            {stopping ? "停止中…" : hotkey ? `停止錄影 (${hotkeyLabel(hotkey)})` : "停止錄影"}
          </button>
        </div>
      );
    }
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
      <button
        className="btn sm"
        disabled={!status}
        onClick={onRecord}
        title={hotkey ? `在任何視窗按 ${hotkeyLabel(hotkey)} 可以直接錄那個視窗` : undefined}
      >
        手動錄影{hotkey && ` (${hotkeyLabel(hotkey)})`}
      </button>
    </div>
  );
}
