// Preview straight from the recording, no image files: a muted <video>
// parked on one frame (media fragment "#t="), loaded only once the card
// scrolls into view. Hovering plays from `at` (up to `end`, if given).
import { useEffect, useRef, useState } from "react";
import { fileUrl } from "../api";

export function VideoThumb({
  path,
  at,
  end,
  hovered,
  disabled,
}: {
  path: string;
  at: number;
  end?: number;
  hovered: boolean;
  disabled?: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const video = useRef<HTMLVideoElement>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const el = box.current;
    if (!el || disabled) return;
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        setVisible(true);
        io.disconnect();
      }
    });
    io.observe(el);
    return () => io.disconnect();
  }, [disabled]);

  useEffect(() => {
    const v = video.current;
    if (!v) return;
    if (hovered) {
      v.currentTime = at;
      void v.play().catch(() => {});
    } else {
      v.pause();
      v.currentTime = at;
    }
  }, [hovered, at]);

  return (
    <div ref={box} style={{ position: "absolute", inset: 0 }}>
      {visible && !disabled ? (
        <video
          ref={video}
          src={`${fileUrl(path)}#t=${at.toFixed(2)}`}
          preload="metadata"
          muted
          playsInline
          onTimeUpdate={(e) => {
            if (end !== undefined && e.currentTarget.currentTime >= end) e.currentTarget.currentTime = at;
          }}
        />
      ) : (
        <div className="placeholder">{disabled ? "錄影 / 處理中" : ""}</div>
      )}
    </div>
  );
}
