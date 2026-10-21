// Settings, saved automatically as they change.
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import type { AudioMode, Config } from "../api";
import { api } from "../api";
import { Calibration } from "./Calibration";
import { IconFolder } from "./Icons";

const AUDIO: { mode: AudioMode; title: string; desc: string }[] = [
  { mode: "game", title: "只錄遊戲聲音", desc: "只抓 Valorant 的聲音,Discord、音樂不會錄進去(建議)" },
  { mode: "system", title: "系統所有聲音", desc: "喇叭 / 耳機聽到的全部,包含語音和音樂" },
  { mode: "game_and_mic", title: "遊戲聲音 + 麥克風", desc: "遊戲聲音之外,也錄你的麥克風" },
  { mode: "off", title: "不錄聲音", desc: "只錄畫面" },
];

export function SettingsView({
  notify,
  onSaved,
}: {
  notify: (message: string, kind?: "error" | "success") => void;
  onSaved: () => void;
}) {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [dir, setDir] = useState("");
  const [ffmpeg, setFfmpeg] = useState<boolean | null>(null);
  const [encoder, setEncoder] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const first = useRef(true);

  useEffect(() => {
    api.getConfig().then(setCfg).catch((e) => notify(String(e), "error"));
    api.ffmpegAvailable().then(setFfmpeg);
    api.encoderName().then(setEncoder).catch(() => {});
  }, [notify]);

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

  async function pickDir() {
    const picked = await open({ directory: true, defaultPath: dir });
    if (typeof picked === "string") set("recordings_dir", picked);
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1 className="page-title">設定</h1>
          <div className="page-sub">變更會自動儲存{saved && <span style={{ color: "var(--kill)" }}> · 已儲存</span>}</div>
        </div>
      </div>

      <div className="settings">
        <div className="section-title">錄影</div>
        <div className="section">
          <div className="audio-options">
            {AUDIO.map((a) => (
              <button
                key={a.mode}
                className={`audio-option ${cfg.audio_mode === a.mode ? "on" : ""}`}
                onClick={() => set("audio_mode", a.mode)}
              >
                <b>{a.title}</b>
                <span>{a.desc}</span>
              </button>
            ))}
          </div>
          <div className="field">
            <div className="field-label">
              <b>影片編碼器</b>
              <span>
                {encoder === "h264_nvenc"
                  ? "NVIDIA 顯示卡硬體編碼,幾乎不影響遊戲效能"
                  : encoder
                    ? "自動選擇的編碼器"
                    : "偵測中…(需要 ffmpeg)"}
              </span>
            </div>
            <span className="chip">{encoder ?? "—"}</span>
          </div>
          <div className="field">
            <div className="field-label">
              <b>畫面更新率</b>
              <span>60 FPS 畫面較順,30 FPS 檔案較小</span>
            </div>
            <div className="segmented">
              {[30, 60].map((f) => (
                <button key={f} className={cfg.video_fps === f ? "on" : ""} onClick={() => set("video_fps", f)}>
                  {f} FPS
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <div className="field-label">
              <b>畫質(位元率)</b>
              <span>越高越清楚,檔案也越大。1440p 建議 20 Mbps 以上</span>
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
          <div className="field">
            <div className="field-label">
              <b>錄影位置</b>
              <span>整場錄影存在這裡;手動匯出的片段在底下的 clips 資料夾</span>
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

        <div className="section-title">精華區間</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>擊殺前保留</b>
              <span>精華區間從第一次擊殺前幾秒開始;相近的擊殺會合併成多殺</span>
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
              <b>擊殺後保留</b>
              <span>最後一次擊殺後再多錄幾秒</span>
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
                <span>錄影需要 ffmpeg。請在終端機執行 winget install Gyan.FFmpeg 後重新開啟 OwnClips。</span>
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
      </div>
    </div>
  );
}
