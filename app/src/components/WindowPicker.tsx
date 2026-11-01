// Picks the window to record by hand.
import { useEffect, useState } from "react";
import type { WindowInfo } from "../api";
import { api } from "../api";

export function WindowPicker({
  onPick,
  onClose,
  notify,
}: {
  onPick: (w: WindowInfo) => void;
  onClose: () => void;
  notify: (message: string, kind?: "error") => void;
}) {
  const [windows, setWindows] = useState<WindowInfo[] | null>(null);

  useEffect(() => {
    api
      .listWindows()
      .then(setWindows)
      .catch((e) => {
        notify(String(e), "error");
        onClose();
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal window-picker" onClick={(e) => e.stopPropagation()}>
        <div className="modal-bar">
          <b>選擇要錄影的視窗</b>
          <button className="btn sm ghost" style={{ marginLeft: "auto" }} onClick={onClose}>
            取消
          </button>
        </div>
        <div className="window-list">
          {windows?.map((w) => (
            <button key={w.hwnd} className="window-item" onClick={() => onPick(w)}>
              <span className="window-title">{w.title}</span>
              <span className="window-process">{w.process}</span>
            </button>
          ))}
          {windows?.length === 0 && <div className="window-hint">沒有可以錄影的視窗</div>}
        </div>
        <div className="window-hint">最小化的視窗不會列出。錄到你按停止或視窗關閉為止,錄影中不會自動錄對局,也不會偵測擊殺。</div>
      </div>
    </div>
  );
}
