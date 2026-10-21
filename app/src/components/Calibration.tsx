// In-app killfeed calibration: grab a frame of the Valorant window, drag a
// box around the killfeed, and save it as fractions of the window size.
import { useEffect, useRef, useState } from "react";
import type { GameFrame, RegionPct } from "../api";
import { api } from "../api";
import { IconCrop } from "./Icons";

export function Calibration({
  region,
  onSaved,
  notify,
}: {
  region: RegionPct | null;
  onSaved: (r: RegionPct) => void;
  notify: (message: string, kind?: "error" | "success") => void;
}) {
  const [frame, setFrame] = useState<GameFrame | null>(null);
  const [sel, setSel] = useState<RegionPct | null>(region);
  const [busy, setBusy] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const img = useRef<HTMLImageElement>(null);

  async function grab() {
    setBusy(true);
    try {
      setFrame(await api.grabGameFrame());
      setSel(region);
    } catch (e) {
      notify(String(e), "error");
    }
    setBusy(false);
  }

  const pos = (e: React.MouseEvent) => {
    const r = box.current!.getBoundingClientRect();
    return {
      x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)),
      y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)),
    };
  };

  // Zoomed preview of the selected area.
  useEffect(() => {
    const c = canvas.current;
    const i = img.current;
    if (!c || !i || !sel || !frame || !i.complete) return;
    const sw = sel.w * i.naturalWidth;
    const sh = sel.h * i.naturalHeight;
    if (sw < 2 || sh < 2) return;
    const scale = Math.min(360 / sw, 260 / sh, 3);
    c.width = Math.round(sw * scale);
    c.height = Math.round(sh * scale);
    c.getContext("2d")!.drawImage(i, sel.x * i.naturalWidth, sel.y * i.naturalHeight, sw, sh, 0, 0, c.width, c.height);
  }, [sel, frame]);

  async function save() {
    if (!sel) return;
    try {
      await api.saveRegion(sel);
      onSaved(sel);
      notify("已儲存 killfeed 區域", "success");
      setFrame(null);
    } catch (e) {
      notify(String(e), "error");
    }
  }

  return (
    <div style={{ padding: "16px 0" }}>
      <div className="field" style={{ padding: 0, border: "none" }}>
        <div className="field-label">
          <b>Killfeed 區域</b>
          <span>
            {region
              ? `已校準 · 位置 ${(region.x * 100).toFixed(1)}%, ${(region.y * 100).toFixed(1)}% · 大小 ${(region.w * 100).toFixed(1)}% × ${(region.h * 100).toFixed(1)}%`
              : "尚未校準:偵測擊殺前一定要先框出畫面右上角的擊殺提示區域"}
          </span>
        </div>
        <button className="btn" disabled={busy} onClick={grab}>
          {busy ? <span className="spinner" /> : <IconCrop size={15} />} {frame ? "重新擷取" : "擷取遊戲畫面"}
        </button>
      </div>

      {frame && (
        <>
          <div className="warn" style={{ marginTop: 12 }}>
            在畫面上拖曳,框住右上角的擊殺提示區(多框幾行的高度,讓 5~6 行擊殺都在框內)。
          </div>
          <div
            ref={box}
            className="calib"
            onMouseDown={(e) => {
              const p = pos(e);
              drag.current = p;
              setSel({ x: p.x, y: p.y, w: 0, h: 0 });
            }}
            onMouseMove={(e) => {
              if (!drag.current) return;
              const p = pos(e);
              const s = drag.current;
              setSel({ x: Math.min(s.x, p.x), y: Math.min(s.y, p.y), w: Math.abs(p.x - s.x), h: Math.abs(p.y - s.y) });
            }}
            onMouseUp={() => (drag.current = null)}
            onMouseLeave={() => (drag.current = null)}
          >
            <img ref={img} src={frame.image} alt="" draggable={false} onLoad={() => setSel((s) => (s ? { ...s } : s))} />
            {sel && (
              <div
                className="sel"
                style={{ left: `${sel.x * 100}%`, top: `${sel.y * 100}%`, width: `${sel.w * 100}%`, height: `${sel.h * 100}%` }}
              />
            )}
          </div>
          <div className="calib-zoom">
            <canvas ref={canvas} />
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <span style={{ color: "var(--muted)", fontSize: 13 }}>
                左邊是放大預覽,確認擊殺提示完整在框內。
              </span>
              <div style={{ display: "flex", gap: 8 }}>
                <button className="btn primary" disabled={!sel || sel.w < 0.02 || sel.h < 0.02} onClick={save}>
                  儲存區域
                </button>
                <button className="btn ghost" onClick={() => setFrame(null)}>
                  取消
                </button>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
