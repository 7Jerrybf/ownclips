# OwnClips 開發文件

使用說明在 [README.md](README.md)。

## 運作方式

| 功能 | 做法 |
|---|---|
| 錄影 | ffmpeg 的 gfxcapture（Windows.Graphics.Capture）以螢幕原生速度擷取遊戲視窗，重新取樣成穩定的 60fps，交給顯卡硬體編碼（NVENC；沒有 NVIDIA 顯卡時改用 AMF／QSV／x264）。錄影中先寫成 MKV，當機也不會壞檔 |
| 聲音 | WASAPI 擷取，每個聲音封包依它的系統時間戳記放到正確位置（跟畫面擷取用同一個時鐘），對局結束後跟畫面對齊、合成 MP4，誤差約 ±10ms |
| 對局開始／結束 | 讀 Valorant 自己寫的記錄檔 `%LOCALAPPDATA%\VALORANT\Saved\Logs\ShooterGame*.log`，地圖和模式也從這裡取得 |
| 擊殺／死亡／助攻 | 看 killfeed 畫面：Valorant 會在你自己的頭像外框一圈黃線。黃框在左邊是你擊殺，在右邊是你死亡；隊友擊殺那一行最左邊的助攻小頭像有黃框，就是你的助攻 |
| 英雄聯盟 | 對局中遊戲本身會在本機開一個 Live Client Data API（`https://127.0.0.1:2999/liveclientdata/allgamedata`，Riot 官方提供給第三方工具用的，不需要金鑰也不連網路）。對局開始／結束、地圖、模式，以及每次擊殺的擊殺者、被擊殺者和助攻者都從這裡讀，不用看畫面，也不需要校準 |
| 其他遊戲 | 每秒看一次最前面的視窗是哪個程式（只問程式路徑，跟工作管理員一樣）：裝在 Steam／Epic／Xbox／GOG／Ubisoft／EA 遊戲庫資料夾裡的、以獨占全螢幕執行的，或在設定裡指定的，就當成遊戲 |
| 精華與預覽 | 精華區間由擊殺時間即時計算；預覽直接用影片本身的畫面，不產生縮圖檔。匯出片段時 ffmpeg 直接複製串流，不重新編碼，很快且畫質無損 |

整個過程不讀寫遊戲記憶體、不注入程式，也不呼叫 Riot 的線上 API（英雄聯盟只讀遊戲自己在本機開的那個 API）。

## 開發需求

- 含 gfxcapture 濾鏡的新版 ffmpeg（winget 上的 Gyan 版本就有），開發時錄影和匯出片段都要用到；打包時也會從這裡複製進安裝檔：

  ```
  winget install Gyan.FFmpeg
  ```

- [Rust](https://rustup.rs)
- Node.js 18 以上

## 編譯與執行

```
cd app
npm install
npx tauri build --no-bundle
```

編譯好的程式在 `target\release\ownclips-app.exe`，也可以直接雙擊根目錄的 `OwnClips.bat`。

打包成安裝檔並發佈（產生 `target\release\bundle\nsis\OwnClips_<版本>_x64-setup.exe`，約 64 MB，上傳到 GitHub 的 `setup` Release）：

```
powershell -ExecutionPolicy Bypass -File app\scripts\release.ps1 -Notes "這版改了什麼"
```

腳本會問簽章私鑰的密碼，打包並簽章後上傳安裝檔、`.sig` 簽章檔和 `latest.json`，並刪掉 Release 上的舊版安裝檔。使用者的 app 會讀 `latest.json` 來發現新版本，並用 `tauri.conf.json` 裡的公鑰驗證安裝檔。私鑰放在 `%USERPROFILE%\.tauri\ownclips.key`，不進 git；**私鑰遺失的話，已安裝的使用者就收不到自動更新**，請另外備份。發佈前記得先升版本號（`app/package.json`、`app/package-lock.json`、`app/src-tauri/Cargo.toml`、`app/src-tauri/tauri.conf.json`），版本號沒變的話使用者不會收到更新。

打包前會自動執行 `app/scripts/fetch-ffmpeg.ps1`，把 winget 裝的 ffmpeg 複製到 `app/src-tauri/resources/ffmpeg/`（檔案太大，不放進 git）；電腦上沒有的話會先用 winget 安裝。安裝檔也內嵌了 WebView2 安裝程式，並設定為安裝給所有使用者（`C:\Program Files\OwnClips`，需要系統管理員權限）。0.5.0 以前是裝在 `%LOCALAPPDATA%\OwnClips`，`app/src-tauri/installer-hooks.nsh` 會在安裝時移除那份舊版。

ffmpeg 是 GPL 授權，授權檔會一起放進安裝檔；公開散布安裝檔時，也要提供 ffmpeg 原始碼的取得方式（例如連到 https://www.gyan.dev/ffmpeg/builds/ ）。

開發模式（前端改了會即時更新）：

```
cd app
npx tauri dev
```

## 設定

| 項目 | 選項 | 預設 |
|---|---|---|
| 聲音 | 只錄遊戲聲音／系統所有聲音／遊戲聲音＋麥克風／不錄 | 只錄遊戲聲音 |
| 畫面更新率 | 30／60 FPS | 60 |
| 畫質 | 6～60 Mbps | 20 Mbps |
| 錄影位置 | 任意資料夾 | `影片\OwnClips` |
| 精華區間：擊殺前／後保留 | 2～20 秒／1～10 秒 | 8 秒／3 秒 |
| Killfeed 區域 | 在 app 裡框選 | 16:9、預設 HUD 的位置 |

設定存在 `%APPDATA%\OwnClips\config.json`，在 app 裡修改會自動儲存。

## 檔案結構

錄影資料夾裡的內容：

```
2026-10-21 19-29-38.mp4            整場錄影
2026-10-21 19-29-38.events.json    這場的地圖、模式和每個事件的時間點
clips\
  2026-10-21 19-29-38 05m10s 精選.mp4  手動匯出的片段（從對局第 5:10 開始）
```

## 命令列工具

不開 app 也能用：

```
cargo build --release -p ownclips

target\release\ownclips.exe detect                  自動錄影（等於 app 的背景功能）
target\release\ownclips.exe clip                    把所有錄影的擊殺片段批次裁成檔案（app 不會自動做）
target\release\ownclips.exe record 10 VALORANT game 錄任意視窗 10 秒，測試錄影器
```

`開始偵測.bat` 和 `裁切片段.bat` 分別對應 `detect` 和 `clip`。

## 專案結構

```
src/                 核心函式庫（app 和命令列共用）
  recorder.rs        內建錄影器
  audio.rs           聲音擷取與混音
  gamestate.rs       讀 Valorant 記錄檔，判斷對局開始／結束
  league.rs          讀英雄聯盟的本機 API：對局開始／結束和擊殺／死亡／助攻
  gamedetect.rs      判斷最前面的視窗是不是其他遊戲
  killfeed.rs        從 killfeed 畫面判斷擊殺／死亡／助攻
  session.rs         自動錄影流程，產生 .events.json
  clip.rs            用 ffmpeg 匯出片段
  capture.rs         視窗擷取
  config.rs          設定
  main.rs            命令列工具
app/                 Tauri 桌面 app
  src-tauri/         後端，提供給前端呼叫的指令
  src/               前端（React + TypeScript）
  scripts/           打包前準備 ffmpeg 的腳本
```

## 已知限制

- **對局結束後要等一下才看得到影片**：要先把畫面和聲音合成 MP4，36 分鐘的對局大約十幾秒到半分鐘。當機留下的錄影，下次開 app 時會自動合成救回。
- **助攻偵測的樣本還很少**：目前只用一場競技的 3 次助攻驗證過。偵測不到的黃色畫面會存到 `%APPDATA%\OwnClips\assist_samples\` 供之後調整。
- **偵測以 1440p 調校**：其他解析度理論上可以用（座標都用相對比例），但還沒實測過；非 16:9 的螢幕需要自己校準。
- **安裝檔沒有數位簽章**：安裝時 Windows 會跳出 SmartScreen 警告。
- **還沒做**：縮到系統匣常駐、開機自動啟動。
