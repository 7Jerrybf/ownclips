// Typed wrappers around the Rust backend commands (app/src-tauri/src/main.rs).
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export type EventKind = "kill" | "death" | "assist";
export type Game = "valorant" | "league";
export type AudioMode = "game" | "system" | "game_and_mic" | "off";
export type VideoCodec = "av1" | "h264";
export type VideoQuality = "high" | "standard" | "small";

export interface MatchEvent {
  t: number;
  kind: EventKind;
}

export interface MatchInfo {
  game: Game;
  map: string;
  mode: string;
  started_at: string;
  duration: number | null;
  events: MatchEvent[];
  keep?: boolean;
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
  video_codec: VideoCodec;
  video_quality: VideoQuality;
  video_bitrate_mbps: number;
  close_to_tray: boolean;
  storage_limit_gb: number | null;
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

export interface StorageUsage {
  used: number;
  delete_count: number;
  delete_bytes: number;
}

export interface GameFrame {
  image: string;
  width: number;
  height: number;
}

export interface UpdateInfo {
  version: string;
  notes: string | null;
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
  encoderName: (codec: VideoCodec) => invoke<string | null>("encoder_name", { codec }),
  listMatches: () => invoke<MatchSummary[]>("list_matches"),
  deleteMatches: (videos: string[]) => invoke<number>("delete_matches", { videos }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  cutRange: (video: string, start: number, end: number) => invoke<string>("cut_range", { video, start, end }),
  reanalyze: (video: string) => invoke<void>("reanalyze_match", { video }),
  storageUsage: (limitGb: number | null) => invoke<StorageUsage>("storage_usage", { limitGb }),
  cleanStorage: () => invoke<number>("clean_storage"),
  setKeep: (video: string, keep: boolean) => invoke<void>("set_keep", { video, keep }),
  grabGameFrame: () => invoke<GameFrame>("grab_game_frame"),
  saveRegion: (region: RegionPct) => invoke<void>("save_region", { region }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  checkUpdate: () => invoke<UpdateInfo | null>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
};

/** URL the webview can load a local file from. */
export const fileUrl = (path: string) => convertFileSrc(path);
