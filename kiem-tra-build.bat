@echo off
chcp 65001 >nul
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
set LOG=%~dp0build-log.txt
echo === vlogcut: kiem tra moi truong va build === > "%LOG%"
echo [cong cu] >> "%LOG%"
where cargo node npm ffmpeg ffprobe >> "%LOG%" 2>&1
cargo -V >> "%LOG%" 2>&1
node -v >> "%LOG%" 2>&1
ffmpeg -hide_banner -version 2>&1 | findstr /b "ffmpeg" >> "%LOG%"
ffmpeg -hide_banner -encoders 2>nul | findstr nvenc >> "%LOG%"
echo [npm install] >> "%LOG%"
call npm install --no-audit --no-fund >> "%LOG%" 2>&1
call npm --prefix ui install --no-audit --no-fund >> "%LOG%" 2>&1
echo [ui build] >> "%LOG%"
call npm --prefix ui run build >> "%LOG%" 2>&1
echo [cargo test core] >> "%LOG%"
cargo test -p vlogcut-core >> "%LOG%" 2>&1
echo TEST_EXIT=%ERRORLEVEL% >> "%LOG%"
echo [cargo build app + mcp] >> "%LOG%"
cargo build -p vlogcut-app -p vlogcut-mcp >> "%LOG%" 2>&1
echo BUILD_EXIT=%ERRORLEVEL% >> "%LOG%"
echo === XONG === >> "%LOG%"
