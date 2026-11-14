@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo vlogcut: build ban cai, tao bieu tuong tren Desktop va bat tu dong cap nhat.
echo Lan dau mat khoang 10 phut, cac lan sau 1-3 phut.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0deploy.ps1" -Open -Setup
if errorlevel 1 (
  echo.
  echo Co loi. Xem file deploy-log.txt
)
pause
