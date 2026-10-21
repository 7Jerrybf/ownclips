@echo off
cd /d "%~dp0"
rem 命令列版的自動錄影(平常用 OwnClips.bat 開 app 就好,app 會在背景自動錄影)
target\release\ownclips.exe detect
echo.
echo (程式已結束,按任意鍵關閉視窗)
pause >nul
