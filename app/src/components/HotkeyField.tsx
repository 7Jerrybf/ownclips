// Settings row for the manual recording hotkey: click, then press the keys.
import { useEffect, useState } from "react";
import { api } from "../api";
import { hotkeyLabel } from "../format";

const MODIFIER_CODES = /^(Control|Shift|Alt|Meta|OS)/;

/** The shortcut a key press stands for, or null if it can't be one. */
function shortcutOf(e: KeyboardEvent): string | null {
  // Without Ctrl or Alt it would take over normal typing; F keys are safe alone.
  if (MODIFIER_CODES.test(e.code) || !(e.ctrlKey || e.altKey || /^F\d+$/.test(e.code))) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
  return [...mods, e.code].join("+");
}

export function HotkeyField({
  value,
  onChange,
  notify,
}: {
  value: string | null;
  /** The new hotkey, already saved by the backend. */
  onChange: (hotkey: string | null) => void;
  notify: (message: string, kind?: "error" | "success") => void;
}) {
  const [listening, setListening] = useState(false);

  async function save(hotkey: string | null) {
    try {
      await api.setManualHotkey(hotkey);
      onChange(hotkey);
    } catch (e) {
      notify(String(e), "error");
    }
  }

  useEffect(() => {
    if (!listening) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape") {
        setListening(false);
        return;
      }
      const shortcut = shortcutOf(e);
      if (shortcut) {
        setListening(false);
        void save(shortcut);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [listening]);

  return (
    <div className="field">
      <div className="field-label">
        <b>快捷鍵</b>
        <span>
          在任何視窗(包含全螢幕遊戲)按下就開始錄那個視窗,手動錄影中再按一次停止
          {listening && " · 按下 Ctrl 或 Alt 加一個鍵,或單獨一個 F 鍵;Esc 取消"}
        </span>
      </div>
      <div className="field-control">
        <button className={`btn ${listening ? "primary" : ""}`} onClick={() => setListening(!listening)}>
          {listening ? "請按下按鍵…" : value ? hotkeyLabel(value) : "未設定"}
        </button>
        {value && !listening && (
          <button className="btn ghost" onClick={() => save(null)}>
            停用
          </button>
        )}
      </div>
    </div>
  );
}
