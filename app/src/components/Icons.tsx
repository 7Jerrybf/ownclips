// Small inline SVG icon set (stroke icons, 24px grid).
import type { ReactNode } from "react";

function Svg({ children, size = 18 }: { children: ReactNode; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </svg>
  );
}

type P = { size?: number };

export const IconMatches = (p: P) => (
  <Svg {...p}>
    <rect x="2" y="4" width="20" height="16" rx="3" />
    <path d="M2 9h20M7 4v5M17 4v5" />
  </Svg>
);
export const IconClips = (p: P) => (
  <Svg {...p}>
    <circle cx="6" cy="6" r="3" />
    <circle cx="6" cy="18" r="3" />
    <path d="M20 4 8.1 15.9M14.5 14.5 20 20M8.1 8.1 12 12" />
  </Svg>
);
export const IconSettings = (p: P) => (
  <Svg {...p}>
    <path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6" />
  </Svg>
);
export const IconPlay = (p: P) => (
  <Svg {...p}>
    <path d="M6 4v16l14-8z" fill="currentColor" />
  </Svg>
);
export const IconTrash = (p: P) => (
  <Svg {...p}>
    <path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6" />
  </Svg>
);
export const IconFolder = (p: P) => (
  <Svg {...p}>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
  </Svg>
);
export const IconKill = (p: P) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 2v5M12 17v5M2 12h5M17 12h5" />
  </Svg>
);
export const IconDeath = (p: P) => (
  <Svg {...p}>
    <path d="M12 3a7 7 0 0 0-4 12.7V19h8v-3.3A7 7 0 0 0 12 3z" />
    <circle cx="9.5" cy="11" r="1.2" fill="currentColor" />
    <circle cx="14.5" cy="11" r="1.2" fill="currentColor" />
    <path d="M10 19v2M14 19v2" />
  </Svg>
);
export const IconAssist = (p: P) => (
  <Svg {...p}>
    <path d="M16 11a4 4 0 1 0-8 0M3 21a6 6 0 0 1 9-5.2A6 6 0 0 1 21 21" />
  </Svg>
);
export const IconBack = (p: P) => (
  <Svg {...p}>
    <path d="M15 18l-6-6 6-6" />
  </Svg>
);
export const IconPrev = (p: P) => (
  <Svg {...p}>
    <path d="M19 20 9 12l10-8zM5 19V5" />
  </Svg>
);
export const IconNext = (p: P) => (
  <Svg {...p}>
    <path d="m5 4 10 8-10 8zM19 5v14" />
  </Svg>
);
export const IconRefresh = (p: P) => (
  <Svg {...p}>
    <path d="M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5" />
  </Svg>
);
export const IconScissors = IconClips;
export const IconRec = (p: P) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="6" fill="currentColor" />
  </Svg>
);
/** Bookmark; filled when the match is kept. */
export const IconKeep = ({ filled, ...p }: P & { filled?: boolean }) => (
  <Svg {...p}>
    <path d="M6 3h12v18l-6-4.5L6 21z" fill={filled ? "currentColor" : "none"} />
  </Svg>
);
export const IconCheck = (p: P) => (
  <Svg {...p}>
    <path d="M5 12.5l4.5 4.5L19 7.5" />
  </Svg>
);
export const IconCrop = (p: P) => (
  <Svg {...p}>
    <path d="M6 2v14a2 2 0 0 0 2 2h14M18 22V8a2 2 0 0 0-2-2H2" />
  </Svg>
);
export const IconPause = (p: P) => (
  <Svg {...p}>
    <path d="M6 4h4v16H6zM14 4h4v16h-4z" fill="currentColor" />
  </Svg>
);
/** Circular arrow with a "5": skip 5 seconds back / forward. */
const Skip5 = ({ dir, ...p }: P & { dir: 1 | -1 }) => (
  <Svg {...p}>
    <g transform={dir < 0 ? "matrix(-1 0 0 1 24 0)" : undefined}>
      <path d="M20 12a8 8 0 1 1-2.34-5.66" />
      <path d="M20 3v4h-4" />
    </g>
    <text x="12" y="15.5" fontSize="9" fontWeight="700" textAnchor="middle" fill="currentColor" stroke="none">
      5
    </text>
  </Svg>
);
export const IconBack5 = (p: P) => <Skip5 dir={-1} {...p} />;
export const IconFwd5 = (p: P) => <Skip5 dir={1} {...p} />;
export const IconVolume = (p: P) => (
  <Svg {...p}>
    <path d="M4 9h4l5-4v14l-5-4H4z" />
    <path d="M16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12" />
  </Svg>
);
export const IconMute = (p: P) => (
  <Svg {...p}>
    <path d="M4 9h4l5-4v14l-5-4H4z" />
    <path d="m17 9 5 6M22 9l-5 6" />
  </Svg>
);
export const IconFullscreen = (p: P) => (
  <Svg {...p}>
    <path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5" />
  </Svg>
);
export const IconExitFullscreen = (p: P) => (
  <Svg {...p}>
    <path d="M9 4v5H4M20 9h-5V4M15 20v-5h5M4 15h5v5" />
  </Svg>
);
/** Clip start / end marks: a bracket with a tick. */
export const IconMarkIn = (p: P) => (
  <Svg {...p}>
    <path d="M10 4H6v16h4M14 12h6M17 9l3 3-3 3" />
  </Svg>
);
export const IconMarkOut = (p: P) => (
  <Svg {...p}>
    <path d="M14 4h4v16h-4M4 12h6M7 9l3 3-3 3" />
  </Svg>
);
export const IconX = (p: P) => (
  <Svg {...p}>
    <path d="M6 6l12 12M18 6 6 18" />
  </Svg>
);
