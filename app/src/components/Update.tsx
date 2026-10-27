// App updates: checks GitHub for a new version on start and every few
// hours, and installs it when asked (the backend refuses while recording).
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { Toast, UpdateInfo } from "../api";
import { api } from "../api";

const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;

export interface Updates {
  current: string;
  available: UpdateInfo | null;
  /** Download progress 0..1 while installing. */
  installing: number | null;
  /** Checks now; `manual` reports "up to date" and errors. */
  check: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
}

export function useUpdates(notify: (message: string, kind?: Toast["kind"]) => void): Updates {
  const [current, setCurrent] = useState("");
  const [available, setAvailable] = useState<UpdateInfo | null>(null);
  const [installing, setInstalling] = useState<number | null>(null);

  const check = useCallback(
    async (manual = false) => {
      try {
        const u = await api.checkUpdate();
        setAvailable(u);
        if (manual && !u) notify("已經是最新版本", "success");
      } catch (e) {
        // Offline or GitHub unreachable: only worth saying when asked.
        if (manual) notify(`檢查更新失敗: ${e}`, "error");
      }
    },
    [notify],
  );

  useEffect(() => {
    getVersion().then(setCurrent);
    check();
    const t = setInterval(() => check(), CHECK_EVERY_MS);
    const unlisten = listen<number>("update-progress", (e) => setInstalling(e.payload));
    return () => {
      clearInterval(t);
      unlisten.then((f) => f());
    };
  }, [check]);

  const install = useCallback(async () => {
    setInstalling(0);
    try {
      // On success the installer closes the app, so this doesn't return.
      await api.installUpdate();
    } catch (e) {
      notify(String(e), "error");
      setInstalling(null);
    }
  }, [notify]);

  return { current, available, installing, check, install };
}

export function UpdateCard({ updates, recording }: { updates: Updates; recording: boolean }) {
  const { available, installing, install } = updates;
  if (!available) return null;
  return (
    <div className="status-card update-card">
      <div className="status-head">
        <span className="dot update" /> 有新版本 {available.version}
      </div>
      {available.notes && <div className="status-sub">{available.notes}</div>}
      <button className="btn sm primary" disabled={recording || installing !== null} onClick={install}>
        {installing !== null
          ? `下載中… ${Math.round(installing * 100)}%`
          : recording
            ? "對局結束後可以更新"
            : "更新並重新開啟"}
      </button>
    </div>
  );
}
