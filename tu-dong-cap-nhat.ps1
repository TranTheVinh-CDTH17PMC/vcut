# vlogcut - trinh tu cap nhat chay nen.
#
# Cu 5 giay kiem tra file .deploy-request trong thu muc project. Khi file nay xuat hien
# (Claude ghi sau khi code xong), chay deploy.ps1 de build va cai ban moi.
# App dang mo se hien nut "Co ban moi - Mo lai".
#
# Duoc deploy.ps1 -Setup dat vao thu muc Startup nen tu chay moi khi mo may.
# Moi lan chi chay mot ban (dung mutex).

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$request = Join-Path $root '.deploy-request'
$heartbeat = Join-Path $root 'deploy-watcher.txt'

$mutex = New-Object System.Threading.Mutex($false, 'Local\vlogcut-tu-dong-cap-nhat')
if (-not $mutex.WaitOne(0)) { exit 0 }

$lastBeat = [DateTime]::MinValue
while ($true) {
    if (((Get-Date) - $lastBeat).TotalSeconds -ge 60) {
        Set-Content -Path $heartbeat -Value (Get-Date).ToString('o') -Encoding ASCII -ErrorAction SilentlyContinue
        $lastBeat = Get-Date
    }
    if (Test-Path $request) {
        # Cho them vai giay de cac file code chep xong het roi moi build.
        Start-Sleep -Seconds 5
        Remove-Item $request -Force -ErrorAction SilentlyContinue
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'deploy.ps1') -Quiet
    }
    Start-Sleep -Seconds 5
}
