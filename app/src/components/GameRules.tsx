// Settings rows for the programs the user marked as a game to record, or as
// one not to: for games the detection misses, and programs it gets wrong.
import { useEffect, useState } from "react";
import type { GameRule, WindowInfo } from "../api";
import { api } from "../api";
import { IconX } from "./Icons";

export function GameRules({
  rules,
  onChange,
  notify,
}: {
  rules: GameRule[];
  onChange: (rules: GameRule[]) => void;
  notify: (message: string, kind?: "error") => void;
}) {
  const [picking, setPicking] = useState(false);

  const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  const setRecord = (exe: string, record: boolean) =>
    onChange(rules.map((r) => (same(r.exe, exe) ? { ...r, record } : r)));

  return (
    <>
      {rules.map((r) => (
        <div className="field" key={r.exe}>
          <div className="field-label">
            <b>{r.exe}</b>
          </div>
          <div className="field-control">
            <div className="segmented">
              <button className={r.record ? "on" : ""} onClick={() => setRecord(r.exe, true)}>
                錄
              </button>
              <button className={r.record ? "" : "on"} onClick={() => setRecord(r.exe, false)}>
                不錄
              </button>
            </div>
            <button
              className="btn ghost icon-btn"
              title="移除(交給自動偵測)"
              onClick={() => onChange(rules.filter((x) => x !== r))}
            >
              <IconX size={15} />
            </button>
          </div>
        </div>
      ))}
      <div className="field">
        <div className="field-label">
          <b>指定程式</b>
          <span>沒被偵測到的遊戲設為「錄」,誤判的程式設為「不錄」</span>
        </div>
        <button className="btn" onClick={() => setPicking(true)}>
          新增…
        </button>
      </div>
      {picking && (
        <ProgramPicker
          taken={rules}
          notify={notify}
          onClose={() => setPicking(false)}
          onPick={(exe) => {
            setPicking(false);
            onChange([...rules, { exe, record: true }]);
          }}
        />
      )}
    </>
  );
}

/** Picks the program of one of the open windows. */
function ProgramPicker({
  taken,
  onPick,
  onClose,
  notify,
}: {
  taken: GameRule[];
  onPick: (exe: string) => void;
  onClose: () => void;
  notify: (message: string, kind?: "error") => void;
}) {
  const [windows, setWindows] = useState<WindowInfo[] | null>(null);

  useEffect(() => {
    api
      .listWindows()
      .then((list) => {
        // One row per program that has no rule yet.
        const seen = new Set(taken.map((r) => r.exe.toLowerCase()));
        setWindows(
          list.filter((w) => {
            const exe = w.process.toLowerCase();
            if (!exe || seen.has(exe)) return false;
            seen.add(exe);
            return true;
          }),
        );
      })
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
          <b>選擇執行中的程式</b>
          <button className="btn sm ghost icon-btn" style={{ marginLeft: "auto" }} title="取消" onClick={onClose}>
            <IconX size={15} />
          </button>
        </div>
        <div className="picker-scroll">
          {windows?.map((w) => (
            <button key={w.hwnd} className="window-item row" title={w.title} onClick={() => onPick(w.process)}>
              <span className="window-title">{w.title}</span>
              <span className="window-process">{w.process}</span>
            </button>
          ))}
          {windows?.length === 0 && <div className="window-hint">先開啟遊戲,再回來新增</div>}
        </div>
      </div>
    </div>
  );
}
