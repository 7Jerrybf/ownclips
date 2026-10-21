# OwnClips (M0 — OBS 連線骨架)

本地化 Valorant 精華片段擷取工具。目前只有 M0:驗證「Rust 程式能控制 OBS 的 Replay Buffer 並存檔」這條地基路徑。完整規劃見 `C:\Users\loe15\.claude\plans\jazzy-snacking-storm.md`。

## 前置需求

1. 安裝 Rust: https://rustup.rs (裝完重開一個終端機,確認 `cargo --version` 有輸出)
2. OBS Studio 28 以上(已內建 obs-websocket,不用裝外掛)

## OBS 設定步驟

1. **開啟 WebSocket Server**
   - OBS 選單 `工具 (Tools)` -> `WebSocket Server Settings`
   - 勾選 `Enable WebSocket server`
   - 確認 Port 是 `4455`(預設值,本程式寫死這個 port)
   - 若有勾 `Enable Authentication`,點 `Show Connect Info` 記下密碼,之後要設環境變數

2. **設定 Replay Buffer**
   - `設定 (Settings)` -> `輸出 (Output)` -> `重播緩衝 (Replay Buffer)` 分頁
   - 勾選 `啟用重播緩衝 (Enable Replay Buffer)`
   - `最長重播時間` 先設 30~60 秒
   - 回到 `輸出 (Output)` -> `錄影 (Recording)` 分頁,確認有設定錄影輸出路徑(Replay Buffer 存檔會存到這裡)

3. 確保 OBS 裡有一個場景/來源在擷取你要錄的畫面(例如一個 Display Capture 或之後會換成只擷取 Valorant 視窗)

## 執行

沒有密碼的話:

```
cargo run
```

有設密碼的話(PowerShell):

```powershell
$env:OBS_WS_PASSWORD = "你的密碼"
cargo run
```

## 驗證 M0 是否成功

1. 程式啟動後應該印出 `Connected. OBS version: ...` 並自動啟動 Replay Buffer(如果還沒啟動)
2. 輸入 `s` + Enter,應該印出 `Saved. Last replay file: ...`
3. 去 OBS 設定的錄影輸出資料夾確認有新的影片檔案出現,長度大約等於你設定的「最長重播時間」
4. 輸入 `q` + Enter 結束程式(Replay Buffer 會留著繼續在 OBS 裡跑)

如果連線失敗,程式會印出對應的檢查提示(WebSocket server 沒開 / port 不對 / 密碼沒設對 / Replay Buffer 沒設定好)。

## 接下來

M0 打通後,下一步(M1)是用 `windows` crate 抓 Valorant 視窗畫面 + 做 killfeed 區域校準工具。
