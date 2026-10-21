# OwnClips

本機執行的 Valorant 精華片段工具，類似 Outplayed，但所有東西都留在你的電腦上，不上傳雲端，也不需要 OBS。

- **自動錄影**：對局開始時自動開錄，結束後自動停止
- **事件偵測**：從 killfeed 畫面判斷你的擊殺和死亡，標記在整場時間軸上
- **自動剪片段**：對局結束後裁出每次擊殺的片段，相近的擊殺會合併成多殺片段
- **內建播放器**：在 app 裡看整場錄影，點時間軸上的標記直接跳到那一刻，也能自己選一段剪下來

## 運作方式

| 功能 | 做法 |
|---|---|
| 錄影 | Windows.Graphics.Capture 擷取 Valorant 視窗，Media Foundation 硬體編碼 H.264；聲音用 WASAPI 擷取 |
| 對局開始／結束 | 讀 Valorant 自己寫的記錄檔 `%LOCALAPPDATA%\VALORANT\Saved\Logs\ShooterGame.log`，地圖和模式也從這裡取得 |
| 擊殺／死亡 | 看 killfeed 畫面：Valorant 會在你自己的頭像外框一圈黃線。黃框在左邊是你擊殺，在右邊是你死亡 |
| 裁切片段 | ffmpeg 直接複製串流，不重新編碼，所以很快、畫質無損 |

整個過程不讀寫遊戲記憶體、不注入程式，也不呼叫 Riot API。原理跟 OBS、Discord 的螢幕錄影一樣。

## 需求

- Windows 10 2004 以上或 Windows 11（「只錄遊戲聲音」需要這個版本以上）
- ffmpeg，裁切片段和產生縮圖要用到：

  ```
  winget install Gyan.FFmpeg
  ```

開發還需要：

- [Rust](https://rustup.rs)
- Node.js 18 以上

## 編譯與執行

```
cd app
npm install
npx tauri build --no-bundle
```

編譯好的程式在 `target\release\ownclips-app.exe`，也可以直接雙擊根目錄的 `OwnClips.bat`。

開發模式（前端改了會即時更新）：

```
cd app
npx tauri dev
```

## 第一次使用

1. 開啟 Valorant，進到任何畫面都可以（大廳也行）。
2. 開 OwnClips →「設定」→「偵測」→「擷取遊戲畫面」，在截圖上框出右上角的 killfeed 區域，按「儲存區域」。框的高度要夠放 5～6 行擊殺提示。
3. 照需要調整錄影設定：聲音來源、畫面更新率、畫質、存放位置。
4. 讓 OwnClips 開著，去打一場。

換了解析度或 HUD 設定後，要重新校準一次。

## 設定

| 項目 | 選項 | 預設 |
|---|---|---|
| 聲音 | 只錄遊戲聲音／系統所有聲音／遊戲聲音＋麥克風／不錄 | 只錄遊戲聲音 |
| 畫面更新率 | 30／60 FPS | 60 |
| 畫質 | 6～60 Mbps | 20 Mbps |
| 錄影位置 | 任意資料夾 | `影片\OwnClips` |
| 自動裁出擊殺片段 | 開／關 | 開 |
| 擊殺前／後保留 | 2～20 秒／1～10 秒 | 8 秒／3 秒 |

設定存在 `%APPDATA%\OwnClips\config.json`，在 app 裡修改會自動儲存。

## 檔案結構

錄影資料夾裡的內容：

```
2026-10-21 19-29-38.mp4            整場錄影
2026-10-21 19-29-38.events.json    這場的地圖、模式和每個事件的時間點
clips\
  2026-10-21 19-29-38 03m27s 4殺.mp4   自動裁出的擊殺片段（從對局第 3:27 開始，4 殺）
  2026-10-21 19-29-38 05m10s 精選.mp4  在時間軸上自己剪的片段
.thumbs\                           縮圖快取
```

## 命令列工具

不開 app 也能用：

```
cargo build --release -p ownclips

target\release\ownclips.exe detect                  自動錄影（等於 app 的背景功能）
target\release\ownclips.exe clip                    幫所有錄影補裁擊殺片段
target\release\ownclips.exe record 10 VALORANT game 錄任意視窗 10 秒，測試錄影器
```

`開始偵測.bat` 和 `裁切片段.bat` 分別對應 `detect` 和 `clip`。

## 專案結構

```
src/                 核心函式庫（app 和命令列共用）
  recorder.rs        內建錄影器
  audio.rs           聲音擷取與混音
  gamestate.rs       讀遊戲記錄檔，判斷對局開始／結束
  killfeed.rs        從 killfeed 畫面判斷擊殺／死亡
  session.rs         自動錄影流程，產生 .events.json
  clip.rs            用 ffmpeg 裁切片段、擷取縮圖
  capture.rs         視窗擷取
  config.rs          設定
  main.rs            命令列工具
app/                 Tauri 桌面 app
  src-tauri/         後端，提供給前端呼叫的指令
  src/               前端（React + TypeScript）
```

## 已知限制

- **還不會偵測助攻**：目前還沒有助攻在 killfeed 上長什麼樣子的樣本。打一般或競技模式時，疑似助攻的畫面會存到 `%APPDATA%\OwnClips\assist_samples\`，之後用這些樣本來實作。
- **錄到一半當機的話，那支影片會打不開**：MP4 要正常收尾才能播放。正常關閉 app 或對局結束都會正確收尾。
- **擊殺偵測以 1440p 調校**：其他解析度理論上可以用（座標都用相對比例），但還沒實測過。
- **還沒做**：縮到系統匣常駐、開機自動啟動、安裝檔。
