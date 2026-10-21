// Typed wrappers around the Rust backend commands (app/src-tauri/src/main.rs).
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export type EventKind = "kill" | "death" | "assist";
export type AudioMode = "game" | "system" | "game_and_mic" | "off";

export interface MatchEvent {
  t: number;
  kind: EventKind;
}

export interface MatchInfo {
  map: string;
  mode: string;
  started_at: string;
  duration: number | null;
  events: MatchEvent[];
}

export interface MatchSummary {
  video: string;
  name: string;
  info: MatchInfo;
  size: number;
  recording: boolean;
}

export interface RegionPct {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Config {
  killfeed_region: RegionPct | null;
  recordings_dir: string | null;
  clip_pre_seconds: number;
  clip_post_seconds: number;
  audio_mode: AudioMode;
  video_fps: number;
  video_bitrate_mbps: number;
}

export interface RecorderStatus {
  state: "waiting" | "recording";
  map: string | null;
  mode: string | null;
  video: string | null;
  started_at: number | null;
  kills: number;
  deaths: number;
  assists: number;
  last_error: string | null;
}

export interface GameFrame {
  image: string;
  width: number;
  height: number;
}

export interface Toast {
  kind: "start" | "stop" | "info" | "error" | "success";
  message: string;
}

export const api = {
  status: () => invoke<RecorderStatus>("get_status"),
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (cfg: Config) => invoke<void>("save_config", { cfg }),
  recordingsDir: () => invoke<string>("recordings_dir"),
  ffmpegAvailable: () => invoke<boolean>("ffmpeg_available"),
  encoderName: () => invoke<string | null>("encoder_name"),
  listMatches: () => invoke<MatchSummary[]>("list_matches"),
  deleteMatch: (video: string) => invoke<void>("delete_match", { video }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  cutRange: (video: string, start: number, end: number) => invoke<string>("cut_range", { video, start, end }),
  grabGameFrame: () => invoke<GameFrame>("grab_game_frame"),
  saveRegion: (region: RegionPct) => invoke<void>("save_region", { region }),
};

/** URL the webview can load a local file from. */
export const fileUrl = (path: string) => convertFileSrc(path);
