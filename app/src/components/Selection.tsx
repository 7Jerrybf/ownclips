// Picking several cards to delete at once: always-visible day checkboxes,
// a checkbox on each card, and a floating bar once anything is picked.
import { useState } from "react";
import { IconCheck, IconTrash, IconX } from "./Icons";

export type CheckState = "all" | "some" | "none";

export function useSelection() {
  const [picked, setPicked] = useState<Set<string>>(new Set());

  function set(ids: string[], on: boolean) {
    setPicked((prev) => {
      const next = new Set(prev);
      ids.forEach((id) => (on ? next.add(id) : next.delete(id)));
      return next;
    });
  }

  function state(ids: string[]): CheckState {
    const n = ids.filter((id) => picked.has(id)).length;
    return n === 0 || ids.length === 0 ? "none" : n === ids.length ? "all" : "some";
  }

  /** Clicking the box: pick all of `ids`, or clear them when all are picked. */
  function toggle(ids: string[]) {
    set(ids, state(ids) !== "all");
  }

  return { picked, set, state, toggle, clear: () => setPicked(new Set()) };
}

export function Check({
  state,
  onClick,
  label,
  title,
}: {
  state: CheckState;
  onClick?: () => void;
  label?: string;
  title?: string;
}) {
  return (
    <button
      type="button"
      className={`check ${state}`}
      role="checkbox"
      aria-checked={state === "all" ? true : state === "some" ? "mixed" : false}
      title={title}
      onClick={
        onClick &&
        ((e) => {
          e.stopPropagation();
          onClick();
        })
      }
    >
      <span className="box">{state === "all" ? <IconCheck size={12} /> : state === "some" ? "–" : null}</span>
      {label}
    </button>
  );
}

/** Floating bar shown while something is picked. */
export function SelectBar({
  count,
  detail,
  allState,
  onAll,
  confirming,
  busy,
  warning,
  onDelete,
  onConfirm,
  onCancel,
  onClear,
}: {
  count: number;
  detail?: string;
  allState: CheckState;
  onAll: () => void;
  confirming: boolean;
  busy: boolean;
  warning?: string | null;
  onDelete: () => void;
  onConfirm: () => void;
  onCancel: () => void;
  onClear: () => void;
}) {
  if (count === 0) return null;
  return (
    <div className="select-float">
      <Check state={allState} onClick={onAll} label="全選" />
      <span className="num">
        <b>{count}</b>
        {detail && <span className="card-meta"> · {detail}</span>}
      </span>
      {confirming ? (
        <>
          {warning && <span className="warn">{warning}</span>}
          <button className="btn sm danger" disabled={busy} onClick={onConfirm}>
            <IconTrash size={15} /> {busy ? "…" : `刪除 ${count}`}
          </button>
          <button className="btn sm ghost" disabled={busy} onClick={onCancel}>
            取消
          </button>
        </>
      ) : (
        <button className="btn sm danger" onClick={onDelete}>
          <IconTrash size={15} /> 刪除
        </button>
      )}
      <button className="btn sm ghost icon-btn" title="取消選取" onClick={onClear}>
        <IconX size={15} />
      </button>
    </div>
  );
}
