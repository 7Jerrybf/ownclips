import { useEffect } from "react";
import { api, fileUrl } from "../api";
import { IconFolder } from "./Icons";

export function ClipModal({ path, title, onClose }: { path: string; title: string; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <video src={fileUrl(path)} controls autoPlay />
        <div className="modal-bar">
          <b>{title}</b>
          <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>
            <button className="btn sm" onClick={() => api.reveal(path)}>
              <IconFolder size={15} /> 顯示位置
            </button>
            <button className="btn sm" onClick={() => api.openPath(path)}>
              用預設播放器開啟
            </button>
            <button className="btn sm ghost" onClick={onClose}>
              關閉 <span className="kbd">Esc</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
