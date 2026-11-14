# vlogcut - build ban cai va cap nhat app tren may.
#
#   deploy.ps1            build + cai vao %LOCALAPPDATA%\Programs\vlogcut
#   deploy.ps1 -Open      ... roi mo app
#   deploy.ps1 -Setup     ... va bat trinh tu cap nhat (chay nen, tu bat khi mo may)
#
# Ket qua ghi vao deploy-log.txt va deploy-status.json (cung thu muc).
# File nay chi dung ky tu ASCII de Windows PowerShell 5.1 doc dung.

param([switch]$Open, [switch]$Setup, [switch]$Quiet)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

$log = Join-Path $root 'deploy-log.txt'
$statusFile = Join-Path $root 'deploy-status.json'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\vlogcut'
$mcpInstalled = Join-Path $env:APPDATA 'vlogcut\bin\vlogcut-mcp.exe'
$started = Get-Date

Set-Content -Path $log -Value "=== vlogcut deploy $($started.ToString('yyyy-MM-dd HH:mm:ss')) ===" -Encoding UTF8

function Say($msg) {
    $line = "[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $msg
    Add-Content -Path $log -Value $line -Encoding UTF8
    if (-not $Quiet) { Write-Host $line }
}

function Write-Status($ok, $msg) {
    $s = [ordered]@{
        ok       = $ok
        message  = $msg
        started  = $started.ToString('o')
        finished = (Get-Date).ToString('o')
        seconds  = [int]((Get-Date) - $started).TotalSeconds
    }
    $s | ConvertTo-Json | Set-Content -Path $statusFile -Encoding UTF8
}

# Chay lenh qua cmd de gom ca stdout lan stderr vao log (cargo in tien do ra stderr).
function Step($name, $cmdline) {
    Say $name
    cmd /c "$cmdline >> `"$log`" 2>&1"
    if ($LASTEXITCODE -ne 0) { throw "$name that bai (ma loi $LASTEXITCODE). Xem deploy-log.txt" }
}

# Windows khong cho ghi de file .exe dang chay nhung cho doi ten no:
# doi ten ban cu thanh *.old-<so>.exe roi chep ban moi vao dung ten.
function Replace-Exe($src, $dst) {
    if (Test-Path $dst) {
        Remove-Item $dst -Force -ErrorAction SilentlyContinue
        if (Test-Path $dst) {
            $old = $dst -replace '\.exe$', (".old-{0}.exe" -f (Get-Date).Ticks)
            Rename-Item $dst (Split-Path -Leaf $old) -ErrorAction Stop
        }
    }
    Copy-Item $src $dst -Force -ErrorAction Stop
}

function New-Shortcut($path, $target, $arguments, $icon, $workdir, $style) {
    $shell = New-Object -ComObject WScript.Shell
    $sc = $shell.CreateShortcut($path)
    $sc.TargetPath = $target
    if ($arguments) { $sc.Arguments = $arguments }
    if ($icon) { $sc.IconLocation = $icon }
    if ($workdir) { $sc.WorkingDirectory = $workdir }
    if ($style) { $sc.WindowStyle = $style }
    $sc.Save()
}

try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw "Khong tim thay cargo (Rust)." }
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { throw "Khong tim thay npm (Node.js)." }

    if (-not (Test-Path (Join-Path $root 'node_modules'))) { Step 'npm install' 'npm install --no-audit --no-fund' }
    Step 'Cai thu vien giao dien' 'npm --prefix ui install --no-audit --no-fund'
    Step 'Build giao dien' 'npm --prefix ui run build'
    Step 'Build MCP server' 'cargo build --profile deploy -p vlogcut-mcp'
    Step 'Build app' 'cargo build --profile deploy -p vlogcut-app --features custom-protocol'

    Say "Cai vao $dest"
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    $out = Join-Path $root 'target\deploy'
    Replace-Exe (Join-Path $out 'vlogcut.exe') (Join-Path $dest 'vlogcut.exe')
    Replace-Exe (Join-Path $out 'vlogcut-mcp.exe') (Join-Path $dest 'vlogcut-mcp.exe')
    Copy-Item (Join-Path $root 'src-tauri\icons\icon.ico') (Join-Path $dest 'vlogcut.ico') -Force -ErrorAction SilentlyContinue

    # Ban MCP ma Claude desktop dang dung (neu da bam "Ket noi Claude").
    if (Test-Path $mcpInstalled) {
        Say 'Cap nhat MCP server cho Claude desktop'
        Replace-Exe (Join-Path $out 'vlogcut-mcp.exe') $mcpInstalled
    }

    # Don cac ban cu khong con chay.
    Get-ChildItem -Path $dest, (Split-Path $mcpInstalled) -Filter '*.old-*.exe' -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue

    $exe = Join-Path $dest 'vlogcut.exe'
    $ico = Join-Path $dest 'vlogcut.ico'
    if (-not (Test-Path $ico)) { $ico = "$exe,0" }
    $desktop = [Environment]::GetFolderPath('Desktop')
    $startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
    New-Shortcut (Join-Path $desktop 'vlogcut.lnk') $exe $null $ico $dest $null
    New-Shortcut (Join-Path $startMenu 'vlogcut.lnk') $exe $null $ico $dest $null
    Say 'Da tao bieu tuong vlogcut tren Desktop va Start'

    if ($Setup) {
        $watcher = Join-Path $root 'tu-dong-cap-nhat.ps1'
        $psArgs = "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$watcher`""
        $startup = [Environment]::GetFolderPath('Startup')
        New-Shortcut (Join-Path $startup 'vlogcut-tu-dong-cap-nhat.lnk') 'powershell.exe' $psArgs $null $root 7
        Start-Process powershell.exe -ArgumentList $psArgs -WindowStyle Hidden
        Say 'Da bat trinh tu cap nhat (tu chay moi khi mo may)'
    }

    $secs = [int]((Get-Date) - $started).TotalSeconds
    Write-Status $true "Da cai ban moi ($secs giay)"
    Say "XONG sau $secs giay"

    if ($Open) { Start-Process $exe -WorkingDirectory $dest }
    exit 0
}
catch {
    Say "LOI: $($_.Exception.Message)"
    Write-Status $false $_.Exception.Message
    exit 1
}
