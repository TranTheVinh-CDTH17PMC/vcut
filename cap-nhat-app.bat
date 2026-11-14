@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo vlogcut: build va cai ban moi tu code hien tai.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0deploy.ps1" -Open
if errorlevel 1 (
  echo.
  echo Co loi. Xem file deploy-log.txt
  pause
)
