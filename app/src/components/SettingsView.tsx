// Settings, saved automatically as they change.
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AudioMode, Config, StorageUsage, VideoCodec, VideoQuality } from "../api";
import { api } from "../api";
import { Calibration } from "./Calibration";
import { size } from "../format";
import { IconFolder } from "./Icons";
import type { Updates } from "./Update";

const CODECS: { codec: VideoCodec; label: string; desc: string }[] = [
  { codec: "av1", label: "AV1", desc: "檔案大約只有 H.264 的一半,畫質幾乎一樣(建議)" },
  { codec: "h264", label: "H.264", desc: "所有裝置和播放器都能播,檔案較大" },
];

const QUALITIES: { quality: VideoQuality; label: string }[] = [
  { quality: "high", label: "高" },
  { quality: "standard", label: "標準" },
  { quality: "small", label: "省空間" },
];

const AUDIO: { mode: AudioMode; title: string; desc: string }[] = [
  { mode: "game", title: "只錄遊戲聲音", desc: "只抓遊戲本身的聲音,Discord、音樂不會錄進去(建議)" },
  { mode: "system", title: "系統所有聲音", desc: "喇叭 / 耳機聽到的全部,包含語音和音樂" },
  { mode: "game_and_mic", title: "遊戲聲音 + 麥克風", desc: "遊戲聲音之外,也錄你的麥克風" },
  { mode: "off", title: "不錄聲音", desc: "只錄畫面" },
];

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
  const first = useRef(true);

  useEffect(() => {
    api
      .getConfig()
      .then(setCfg)
      .catch((e) => notify(String(e), "error"));
    api.ffmpegAvailable().then(setFfmpeg);
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
      notify(`已刪除最舊的 ${n} 場錄影`, "success");
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

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1 className="page-title">設定</h1>
          <div className="page-sub">
            變更會自動儲存{saved && <span style={{ color: "var(--kill)" }}> · 已儲存</span>}
          </div>
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
              <b>影片格式</b>
              <span>{CODECS.find((c) => c.codec === cfg.video_codec)?.desc}</span>
            </div>
            <div className="segmented">
              {CODECS.map((c) => (
                <button
                  key={c.codec}
                  className={cfg.video_codec === c.codec ? "on" : ""}
                  onClick={() => set("video_codec", c.codec)}
                >
                  {c.label}
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <div className="field-label">
              <b>影片編碼器</b>
              <span>
                {fallback
                  ? "這張顯示卡不支援 AV1 硬體編碼,會改用 H.264 錄影"
                  : encoder?.endsWith("_nvenc")
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
          {av1 ? (
            <div className="field">
              <div className="field-label">
                <b>畫質</b>
                <span>標準:清晰度和 H.264 20 Mbps 差不多。省空間:檔案再小約三成,細節稍微少一點</span>
              </div>
              <div className="segmented">
                {QUALITIES.map((q) => (
                  <button
                    key={q.quality}
                    className={cfg.video_quality === q.quality ? "on" : ""}
                    onClick={() => set("video_quality", q.quality)}
                  >
                    {q.label}
                  </button>
                ))}
              </div>
            </div>
          ) : (
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
          )}
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

        <div className="section-title">儲存空間</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>容量上限</b>
              <span>
                {usage ? `目前錄影共 ${size(usage.used)}` : "計算中…"}
                {cfg.storage_limit_gb === null
                  ? "。開啟後,超過上限會自動刪除最舊的錄影"
                  : `,超過 ${cfg.storage_limit_gb} GB 時自動刪除最舊的錄影(已保留的對局和匯出的片段不會刪)`}
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
                  下一場對局存好後(或下次開啟 OwnClips 時)會永久刪除最舊的 {usage.delete_count} 場錄影,釋放{" "}
                  {size(usage.delete_bytes)}。想留下的對局請先按「保留」。
                </span>
              </div>
              {confirmClean ? (
                <div className="field-control">
                  <button className="btn danger" onClick={cleanNow}>
                    確定刪除 {usage.delete_count} 場
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

        <div className="section-title">背景執行</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>關閉視窗時縮到系統匣</b>
              <span>按 ✕ 後 OwnClips 繼續在右下角背景偵測、錄影;要完全結束請在系統匣圖示按右鍵</span>
            </div>
            <button
              className={`switch ${cfg.close_to_tray ? "on" : ""}`}
              aria-pressed={cfg.close_to_tray}
              onClick={() => set("close_to_tray", !cfg.close_to_tray)}
            />
          </div>
          <div className="field">
            <div className="field-label">
              <b>開機時自動啟動</b>
              <span>登入 Windows 後直接在背景待命,不會跳出視窗,開打就自動錄</span>
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

        <div className="section-title">關於</div>
        <div className="section">
          <div className="field">
            <div className="field-label">
              <b>OwnClips {updates.current}</b>
              <span>
                {updates.available
                  ? `有新版本 ${updates.available.version},可以在左下角更新`
                  : "啟動時和每隔幾小時會自動檢查新版本"}
              </span>
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
