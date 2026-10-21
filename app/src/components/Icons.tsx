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
export const IconCrop = (p: P) => (
  <Svg {...p}>
    <path d="M6 2v14a2 2 0 0 0 2 2h14M18 22V8a2 2 0 0 0-2-2H2" />
  </Svg>
);
