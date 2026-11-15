// Picks what to record by hand: a whole screen or one window, with a
// picture of each.
import { useEffect, useState } from "react";
import type { MonitorInfo, RecordTarget, WindowInfo } from "../api";
import { api } from "../api";
import { IconX } from "./Icons";

export function SourcePicker({
  onPick,
  onClose,
  notify,
}: {
  onPick: (target: RecordTarget) => void;
  onClose: () => void;
  notify: (message: string, kind?: "error") => void;
}) {
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [windows, setWindows] = useState<WindowInfo[] | null>(null);
  // Keyed by kind + id. null: couldn't be captured; missing: still loading.
  const [previews, setPreviews] = useState<Record<string, string | null>>({});

  useEffect(() => {
    const load = (key: string, image: Promise<string | null>) => {
      const done = (i: string | null) => setPreviews((p) => ({ ...p, [key]: i }));
      image.then(done, () => done(null));
    };
    api
      .listMonitors()
      .then((list) => {
        setMonitors(list);
        list.forEach((m) => load(`monitor${m.hmonitor}`, api.monitorPreview(m.hmonitor)));
      })
      .catch(() => {});
    api
      .listWindows()
      .then((list) => {
        setWindows(list);
        list.forEach((w) => load(`window${w.hwnd}`, api.windowPreview(w.hwnd)));
      })
      .catch((e) => {
        notify(String(e), "error");
        onClose();
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const item = (target: RecordTarget, title: string, sub: string) => {
    const preview = previews[target.kind + target.id];
    return (
      <button key={target.kind + target.id} className="window-item" title={title} onClick={() => onPick(target)}>
        <span className="window-preview">
          {preview ? <img src={preview} alt="" draggable={false} /> : <span>{preview === null ? "—" : "…"}</span>}
        </span>
        <span className="window-title">{title}</span>
        <span className="window-process">{sub}</span>
      </button>
    );
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal window-picker" onClick={(e) => e.stopPropagation()}>
        <div className="modal-bar">
          <b>錄影來源</b>
          <button className="btn sm ghost icon-btn" style={{ marginLeft: "auto" }} title="取消" onClick={onClose}>
            <IconX size={15} />
          </button>
        </div>
        <div className="picker-scroll">
          {monitors.length > 0 && (
            <>
              <div className="picker-group">螢幕</div>
              <div className="window-list">
                {monitors.map((m) =>
                  item({ kind: "monitor", id: m.hmonitor }, `螢幕 ${m.index}`, [m.name, `${m.width} × ${m.height}`].filter(Boolean).join(" · ")),
                )}
              </div>
            </>
          )}
          <div className="picker-group">視窗</div>
          <div className="window-list">
            {windows?.map((w) => item({ kind: "window", id: w.hwnd }, w.title, w.process))}
            {windows?.length === 0 && <div className="window-hint">無視窗</div>}
          </div>
        </div>
      </div>
    </div>
  );
}
