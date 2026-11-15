// Settings, saved automatically as they change.
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AudioDevice, AudioMode, Config, StorageUsage, VideoCodec, VideoQuality } from "../api";
import { api } from "../api";
import { Calibration } from "./Calibration";
import { HotkeyField } from "./HotkeyField";
import { size } from "../format";
import { IconFolder } from "./Icons";
import type { Updates } from "./Update";

const CODECS: { codec: VideoCodec; label: string; hint: string }[] = [
  { codec: "av1", label: "AV1", hint: "檔案約 H.264 一半(建議)" },
  { codec: "h264", label: "H.264", hint: "相容性最好" },
];

const QUALITIES: { quality: VideoQuality; label: string }[] = [
  { quality: "high", label: "高" },
  { quality: "standard", label: "標準" },
  { quality: "small", label: "省空間" },
];

/** null = the game window's own size. */
const RESOLUTIONS: { height: number | null; label: string }[] = [
  { height: null, label: "原始" },
  { height: 1440, label: "1440p" },
  { height: 1080, label: "1080p" },
  { height: 720, label: "720p" },
];

// The backend has one combined mode; the UI splits it into "what you hear"
// plus a separate microphone switch so the mic can't be missed.
type AudioSource = "game" | "system";
const AUDIO: { source: AudioSource; label: string; hint: string }[] = [
  { source: "game", label: "遊戲", hint: "只錄遊戲 / 該視窗的聲音(錄螢幕時為系統聲音)" },
  { source: "system", label: "系統", hint: "電腦所有聲音,含語音、音樂" },
];

function audioSource(mode: AudioMode): AudioSource {
  return mode === "game_and_mic" ? "game" : mode === "system_and_mic" ? "system" : mode;
}

function hasMic(mode: AudioMode): boolean {
  return mode === "game_and_mic" || mode === "system_and_mic";
}

function audioMode(source: AudioSource, mic: boolean): AudioMode {
  if (!mic) return source;
  return source === "game" ? "game_and_mic" : "system_and_mic";
}

export function SettingsView({
  notify,
  onSaved,
  updates,
}: {
  notify: (message: string, kind?: "error" | "success") => void;
  onSaved: () => void;
  updates: Updates;
}) {
  const [checking, setChecking] = useState(false);
  const [cfg, setCfg] = useState<Config | null>(null);
  const [dir, setDir] = useState("");
  const [ffmpeg, setFfmpeg] = useState<boolean | null>(null);
  const [encoder, setEncoder] = useState<string | null>(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [usage, setUsage] = useState<StorageUsage | null>(null);
  const [confirmClean, setConfirmClean] = useState(false);
  const [saved, setSaved] = useState(false);
  const [mics, setMics] = useState<AudioDevice[]>([]);
  const first = useRef(true);

  useEffect(() => {
    api
      .getConfig()
      .then(setCfg)
      .catch((e) => notify(String(e), "error"));
    api.ffmpegAvailable().then(setFfmpeg);
    api
      .listMicrophones()
      .then(setMics)
      .catch(() => {});
    api
      .getAutostart()
      .then(setAutostart)
      .catch(() => {});
  }, [notify]);

  // Which encoder the chosen format ends up using (AV1 falls back to H.264
  // on cards without an AV1 encoder).
  useEffect(() => {
    if (!cfg) return;
    setEncoder(null);
    api
      .encoderName(cfg.video_codec)
      .then(setEncoder)
      .catch(() => {});
  }, [cfg?.video_codec]);

  // Folder usage, and what the limit being set would delete.
  const limitGb = cfg?.storage_limit_gb ?? null;
  const refreshUsage = useCallback(() => {
    api
      .storageUsage(limitGb)
      .then(setUsage)
      .catch(() => {});
  }, [limitGb]);
  useEffect(() => {
    setConfirmClean(false);
    refreshUsage();
  }, [refreshUsage, dir]);

  useEffect(() => {
    api.recordingsDir().then(setDir);
  }, [cfg?.recordings_dir]);

  // Autosave (debounced) after edits.
  useEffect(() => {
    if (!cfg) return;
    if (first.current) {
      first.current = false;
      return;
    }
    const t = setTimeout(() => {
      api
        .saveConfig(cfg)
        .then(() => {
          onSaved();
          setSaved(true);
          setTimeout(() => setSaved(false), 1500);
        })
        .catch((e) => notify(String(e), "error"));
    }, 350);
    return () => clearTimeout(t);
  }, [cfg, notify, onSaved]);

  if (!cfg) return <div className="page" />;
  const set = <K extends keyof Config>(k: K, v: Config[K]) => setCfg({ ...cfg, [k]: v });
  const fallback = cfg.video_codec === "av1" && !!encoder && !encoder.startsWith("av1");
  // Until the encoder is known, show the controls for the chosen format.
  const av1 = cfg.video_codec === "av1" && !fallback;

  async function toggleAutostart() {
    const next = !autostart;
    try {
      await api.setAutostart(next);
      setAutostart(next);
    } catch (e) {
      notify(String(e), "error");
    }
  }

  async function cleanNow() {
    if (!cfg) return;
    try {
      await api.saveConfig(cfg); // the cleanup reads the saved limit
      const n = await api.cleanStorage();
      notify(`已清理 ${n} 場`, "success");
    } catch (e) {
      notify(String(e), "error");
    }
    setConfirmClean(false);
    refreshUsage();
  }

  async function pickDir() {
    const picked = await open({ directory: true, defaultPath: dir });
    if (typeof picked === "string") set("recordings_dir", picked);
  }

  const seg = <T,>(items: { v: T; label: string; title?: string }[], value: T, onPick: (v: T) => void) => (
    <div className="segmented">
      {items.map((i) => (
        <button key={i.label} title={i.title} className={value === i.v ? "on" : ""} onClick={() => onPick(i.v)}>
          {i.label}
        </button>
      ))}
    </div>
  );

  const volume = (key: "audio_volume" | "mic_volume") => (
    <div className="range-wrap">
      <input
        type="range"
        min={0}
        max={200}
        step={5}
        value={cfg[key]}
        onChange={(e) => set(key, Number(e.target.value))}
      />
      <span className="val num">{cfg[key]}%</span>
    </div>
  );

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1 className="page-title">設定</h1>
          <div className="page-sub">{saved ? <span style={{ color: "var(--kill)" }}>✓ 已儲存</span> : "自動儲存"}</div>
        </div>
      </div>

      <div className="settings">
        <div className="section-title">錄影</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>聲音</b>
              <span>{AUDIO.find((a) => a.source === audioSource(cfg.audio_mode))?.hint}</span>
            </div>
            {seg(
              AUDIO.map((a) => ({ v: a.source, label: a.label, title: a.hint })),
              audioSource(cfg.audio_mode),
              (src) => set("audio_mode", audioMode(src, hasMic(cfg.audio_mode))),
            )}
          </div>
          <div className="field">
            <div className="field-label">
              <b>音量</b>
            </div>
            {volume("audio_volume")}
          </div>
          <div className="field">
            <div className="field-label">
              <b>麥克風</b>
            </div>
            <div className="field-control">
              {hasMic(cfg.audio_mode) && (
                <select
                  className="select"
                  value={cfg.mic_device ?? ""}
                  onChange={(e) => set("mic_device", e.target.value || null)}
                >
                  <option value="">系統預設</option>
                  {mics.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name}
                    </option>
                  ))}
                  {cfg.mic_device && !mics.some((m) => m.id === cfg.mic_device) && (
                    <option value={cfg.mic_device}>未連接(改用預設)</option>
                  )}
                </select>
              )}
              <button
                className={`switch ${hasMic(cfg.audio_mode) ? "on" : ""}`}
                aria-pressed={hasMic(cfg.audio_mode)}
                onClick={() =>
                  set("audio_mode", audioMode(audioSource(cfg.audio_mode), !hasMic(cfg.audio_mode)))
                }
              />
            </div>
          </div>
          {hasMic(cfg.audio_mode) && (
            <div className="field">
              <div className="field-label">
                <b>麥克風音量</b>
              </div>
              {volume("mic_volume")}
            </div>
          )}
          <div className="field">
            <div className="field-label">
              <b>格式</b>
              {fallback ? (
                <span className="warn">顯示卡不支援 AV1,改用 H.264</span>
              ) : (
                <span>{CODECS.find((c) => c.codec === cfg.video_codec)?.hint}</span>
              )}
            </div>
            <span
              className="chip"
              title={encoder?.endsWith("_nvenc") ? "NVIDIA 硬體編碼" : encoder ? "編碼器" : "偵測中(需要 ffmpeg)"}
            >
              {encoder ?? "…"}
            </span>
            {seg(
              CODECS.map((c) => ({ v: c.codec, label: c.label, title: c.hint })),
              cfg.video_codec,
              (c) => set("video_codec", c),
            )}
          </div>
          <div className="field">
            <div className="field-label">
              <b>解析度</b>
              <span>高於遊戲解析度時錄原始</span>
            </div>
            {seg(
              RESOLUTIONS.map((r) => ({ v: r.height, label: r.label })),
              cfg.video_height ?? null,
              (h) => set("video_height", h),
            )}
          </div>
          <div className="field">
            <div className="field-label">
              <b>FPS</b>
            </div>
            {seg(
              [30, 60].map((f) => ({ v: f, label: String(f) })),
              cfg.video_fps,
              (f) => set("video_fps", f),
            )}
          </div>
          {av1 ? (
            <div className="field">
              <div className="field-label">
                <b>畫質</b>
              </div>
              {seg(
                QUALITIES.map((q) => ({ v: q.quality, label: q.label })),
                cfg.video_quality,
                (q) => set("video_quality", q),
              )}
            </div>
          ) : (
            <div className="field">
              <div className="field-label">
                <b>位元率</b>
                <span>1440p 建議 ≥ 20 Mbps</span>
              </div>
              <div className="range-wrap">
                <input
                  type="range"
                  min={6}
                  max={60}
                  step={2}
                  value={cfg.video_bitrate_mbps}
                  onChange={(e) => set("video_bitrate_mbps", Number(e.target.value))}
                />
                <span className="val num">{cfg.video_bitrate_mbps} Mbps</span>
              </div>
            </div>
          )}
          <div className="field">
            <div className="field-label">
              <b>位置</b>
            </div>
            <div className="field-control">
              <span className="path-box" title={dir}>
                {dir}
              </span>
              <button className="btn" onClick={pickDir}>
                選擇…
              </button>
              <button className="btn ghost icon-btn" title="開啟資料夾" onClick={() => api.openPath(dir)}>
                <IconFolder size={16} />
              </button>
            </div>
          </div>
        </div>

        <div className="section-title">儲存空間</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>容量上限</b>
              <span title="超過時自動刪除最舊錄影(已保留與片段除外)">
                {usage ? `已用 ${size(usage.used)}` : "…"}
                {cfg.storage_limit_gb !== null && " · 超過自動刪最舊"}
              </span>
            </div>
            <div className="field-control">
              {cfg.storage_limit_gb !== null && (
                <div className="range-wrap">
                  <input
                    type="range"
                    min={20}
                    max={2000}
                    step={10}
                    value={cfg.storage_limit_gb}
                    onChange={(e) => set("storage_limit_gb", Number(e.target.value))}
                  />
                  <span className="val num">{cfg.storage_limit_gb} GB</span>
                </div>
              )}
              <button
                className={`switch ${cfg.storage_limit_gb !== null ? "on" : ""}`}
                aria-pressed={cfg.storage_limit_gb !== null}
                onClick={() => set("storage_limit_gb", cfg.storage_limit_gb === null ? 100 : null)}
              />
            </div>
          </div>
          {usage && usage.delete_count > 0 && (
            <div className="field">
              <div className="field-label">
                <b className="warn">超過上限</b>
                <span>
                  將刪除最舊 {usage.delete_count} 場 · {size(usage.delete_bytes)}
                </span>
              </div>
              {confirmClean ? (
                <div className="field-control">
                  <button className="btn danger" onClick={cleanNow}>
                    刪除 {usage.delete_count} 場
                  </button>
                  <button className="btn ghost" onClick={() => setConfirmClean(false)}>
                    取消
                  </button>
                </div>
              ) : (
                <button className="btn" onClick={() => setConfirmClean(true)}>
                  立即清理
                </button>
              )}
            </div>
          )}
        </div>

        <div className="section-title">錄影快捷鍵</div>
        <div className="section">
          <HotkeyField
            value={cfg.manual_hotkey}
            notify={notify}
            onChange={(hotkey) => {
              first.current = true; // already saved by the backend
              setCfg({ ...cfg, manual_hotkey: hotkey });
              onSaved();
            }}
          />
        </div>

        <div className="section-title">背景執行</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>關閉時縮到系統匣</b>
              <span>右鍵系統匣圖示可完全結束</span>
            </div>
            <button
              className={`switch ${cfg.close_to_tray ? "on" : ""}`}
              aria-pressed={cfg.close_to_tray}
              onClick={() => set("close_to_tray", !cfg.close_to_tray)}
            />
          </div>
          <div className="field">
            <div className="field-label">
              <b>開機自動啟動</b>
            </div>
            <button
              className={`switch ${autostart ? "on" : ""}`}
              aria-pressed={!!autostart}
              disabled={autostart === null}
              onClick={toggleAutostart}
            />
          </div>
        </div>

        <div className="section-title">精華區間</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>擊殺前</b>
            </div>
            <div className="range-wrap">
              <input
                type="range"
                min={2}
                max={20}
                step={1}
                value={cfg.clip_pre_seconds}
                onChange={(e) => set("clip_pre_seconds", Number(e.target.value))}
              />
              <span className="val num">{cfg.clip_pre_seconds} 秒</span>
            </div>
          </div>
          <div className="field">
            <div className="field-label">
              <b>擊殺後</b>
            </div>
            <div className="range-wrap">
              <input
                type="range"
                min={1}
                max={10}
                step={1}
                value={cfg.clip_post_seconds}
                onChange={(e) => set("clip_post_seconds", Number(e.target.value))}
              />
              <span className="val num">{cfg.clip_post_seconds} 秒</span>
            </div>
          </div>
          {ffmpeg === false && (
            <div className="field">
              <div className="field-label">
                <b className="warn">找不到 ffmpeg</b>
                <span>winget install Gyan.FFmpeg</span>
              </div>
            </div>
          )}
        </div>

        <div className="section-title">偵測</div>
        <div className="section">
          <Calibration
            region={cfg.killfeed_region}
            onSaved={(r) => {
              first.current = true; // already saved by the backend
              setCfg({ ...cfg, killfeed_region: r });
            }}
            notify={notify}
          />
        </div>

        <div className="section-title">關於</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>OwnClips {updates.current}</b>
              {updates.available && <span>新版本 {updates.available.version}</span>}
            </div>
            <button
              className="btn sm"
              disabled={checking}
              onClick={async () => {
                setChecking(true);
                await updates.check(true);
                setChecking(false);
              }}
            >
              {checking ? "檢查中…" : "檢查更新"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
