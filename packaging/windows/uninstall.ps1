<#
.SYNOPSIS
    Removes everything install.ps1 created.
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File uninstall.ps1
#>
$ErrorActionPreference = 'Stop'

$target = Join-Path $env:LOCALAPPDATA 'Programs\QCY Earbuds'
$startMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'QCY Earbuds'
$startupLink = Join-Path ([Environment]::GetFolderPath('Startup')) 'QCY Earbuds tray.lnk'

Get-Process qcy-gui, qcy-tray -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

foreach ($item in $startupLink, $startMenu, $target) {
    if (Test-Path $item) { Remove-Item $item -Recurse -Force }
}
# Settings and the endpoint cache.
foreach ($item in (Join-Path $env:APPDATA 'qcy-earbuds'), (Join-Path $env:LOCALAPPDATA 'qcy-earbuds')) {
    if (Test-Path $item) { Remove-Item $item -Recurse -Force }
}

$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($path) {
    $kept = ($path -split ';') | Where-Object { $_ -and $_ -ne $target }
    [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')
}

Write-Host 'QCY Earbuds has been removed.'
