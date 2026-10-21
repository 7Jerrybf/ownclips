// Lazily generated thumbnail: asks the backend (ffmpeg) for a cached frame
// once the card scrolls into view.
import { useEffect, useRef, useState } from "react";
import { api, fileUrl } from "../api";

export function Thumb({ path, at, disabled }: { path: string; at: number; disabled?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (disabled) return;
    const el = ref.current;
    if (!el) return;
    let cancelled = false;
    const io = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      io.disconnect();
      api
        .thumbnail(path, at)
        .then((p) => !cancelled && setSrc(fileUrl(p)))
        .catch(() => !cancelled && setFailed(true));
    });
    io.observe(el);
    return () => {
      cancelled = true;
      io.disconnect();
    };
  }, [path, at, disabled]);

  return (
    <div ref={ref} style={{ position: "absolute", inset: 0 }}>
      {src ? (
        <img src={src} alt="" loading="lazy" />
      ) : (
        <div className="placeholder">{disabled ? "錄影中" : failed ? "無法產生縮圖" : ""}</div>
      )}
    </div>
  );
}
