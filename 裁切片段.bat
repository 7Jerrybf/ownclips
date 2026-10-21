@echo off
cd /d "%~dp0"
rem 請先在 OBS 停止錄影再執行,還在錄的影片無法裁切
target\release\ownclips.exe clip
echo.
echo (程式已結束,按任意鍵關閉視窗)
pause >nul
