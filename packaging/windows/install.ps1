<#
.SYNOPSIS
    Installs QCY Earbuds for the current user (no administrator rights needed).

.DESCRIPTION
    Copies the programs to %LOCALAPPDATA%\Programs\QCY Earbuds and adds Start
    menu shortcuts.

    -Autostart  also start the tray icon when you sign in
    -AddToPath  make `qcyctl` available in new terminals

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1 -Autostart
#>
param(
    [switch]$Autostart,
    [switch]$AddToPath
)
$ErrorActionPreference = 'Stop'

$source = $PSScriptRoot
$target = Join-Path $env:LOCALAPPDATA 'Programs\QCY Earbuds'
$startMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'QCY Earbuds'
$startup = [Environment]::GetFolderPath('Startup')

# Running copies would keep the files locked.
Get-Process qcy-gui, qcy-tray -ErrorAction SilentlyContinue | Stop-Process -Force

New-Item -ItemType Directory -Force -Path $target, $startMenu | Out-Null
foreach ($file in 'qcy-gui.exe', 'qcy-tray.exe', 'qcyctl.exe', 'README.txt') {
    Copy-Item (Join-Path $source $file) $target -Force
}

$shell = New-Object -ComObject WScript.Shell
function New-Shortcut($path, $exe, $description) {
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = Join-Path $target $exe
    $link.WorkingDirectory = $target
    $link.Description = $description
    $link.Save()
}
New-Shortcut (Join-Path $startMenu 'QCY Earbuds.lnk') 'qcy-gui.exe' 'Control your QCY earbuds'
New-Shortcut (Join-Path $startMenu 'QCY Earbuds tray.lnk') 'qcy-tray.exe' 'Tray icon for your QCY earbuds'

if ($Autostart) {
    New-Shortcut (Join-Path $startup 'QCY Earbuds tray.lnk') 'qcy-tray.exe' 'Tray icon for your QCY earbuds'
    Start-Process (Join-Path $target 'qcy-tray.exe')
}

if ($AddToPath) {
    $path = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($path -split ';') -notcontains $target) {
        [Environment]::SetEnvironmentVariable('Path', ($path.TrimEnd(';') + ';' + $target), 'User')
    }
}

Write-Host "Installed to $target"
Write-Host 'Start "QCY Earbuds" from the Start menu.'
