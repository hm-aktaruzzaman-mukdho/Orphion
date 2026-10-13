<#
.SYNOPSIS
    Installs Orphion for the current user (no administrator rights needed).

.DESCRIPTION
    Copies the programs to %LOCALAPPDATA%\Programs\Orphion and adds an
    "Orphion" Start menu shortcut. Starting Orphion also starts its tray icon
    (unless turned off in its settings).

    -Autostart  also start the tray icon when you sign in (and right away)
    -AddToPath  make `orphionctl` available in new terminals

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1 -Autostart
#>
param(
    [switch]$Autostart,
    [switch]$AddToPath
)
$ErrorActionPreference = 'Stop'

$source = $PSScriptRoot
$target = Join-Path $env:LOCALAPPDATA 'Programs\Orphion'
$startMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Orphion'
$startup = [Environment]::GetFolderPath('Startup')

# Running copies would keep the files locked. Stop-Process returns before
# they are gone: wait for them, then drop our own handles to them (an open
# process handle keeps its .exe locked). orphion is the window and the tray;
# orphion-tray is the separate tray of earlier versions, qcy-gui and qcy-tray
# the names from before the rename to Orphion.
$running = @(Get-Process orphion, orphion-tray, qcy-gui, qcy-tray -ErrorAction SilentlyContinue)
$running | Stop-Process -Force
$running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
$running | ForEach-Object { $_.Dispose() }

# Replace a version from before the rename ("QCY Earbuds"), keeping its
# autostart and Path choices. Its settings are picked up by Orphion itself.
$legacyTarget = Join-Path $env:LOCALAPPDATA 'Programs\QCY Earbuds'
$legacyStartMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'QCY Earbuds'
$legacyStartup = Join-Path $startup 'QCY Earbuds tray.lnk'
$legacyKey = Get-Item 'HKCU:\Environment'
$legacyPath = $legacyKey.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
if (Test-Path $legacyStartup) { $Autostart = $true }
if (($legacyPath -split ';') -contains $legacyTarget) {
    $AddToPath = $true
    $kept = ($legacyPath -split ';') | Where-Object { $_ -ne $legacyTarget }
    Set-ItemProperty 'HKCU:\Environment' -Name Path -Value ($kept -join ';') -Type $legacyKey.GetValueKind('Path')
}
foreach ($item in $legacyStartup, $legacyStartMenu, $legacyTarget,
        (Join-Path $env:LOCALAPPDATA 'qcy-earbuds')) {
    for ($i = 1; Test-Path $item; $i++) {
        try {
            Remove-Item $item -Recurse -Force
        } catch {
            if ($i -ge 20) { throw }
            Start-Sleep -Milliseconds 250
        }
    }
}

# Earlier Orphion versions had a separate orphion-tray.exe with its own
# shortcuts. The tray is now `orphion.exe --tray`; an old Startup shortcut
# becomes the new autostart.
$oldTrayStartup = Join-Path $startup 'Orphion tray.lnk'
if (Test-Path $oldTrayStartup) { $Autostart = $true }
foreach ($item in $oldTrayStartup, (Join-Path $startMenu 'Orphion tray.lnk'),
        (Join-Path $target 'orphion-tray.exe')) {
    for ($i = 1; Test-Path $item; $i++) {
        try {
            Remove-Item $item -Force
        } catch {
            if ($i -ge 20) { throw }
            Start-Sleep -Milliseconds 250
        }
    }
}

New-Item -ItemType Directory -Force -Path $target, $startMenu | Out-Null
foreach ($file in 'orphion.exe', 'orphionctl.exe', 'README.txt') {
    # Windows releases the files of a just-ended program with a short delay.
    for ($i = 1; ; $i++) {
        try {
            Copy-Item (Join-Path $source $file) $target -Force
            break
        } catch {
            if ($i -ge 20) { throw }
            Start-Sleep -Milliseconds 250
        }
    }
}

function Send-EnvironmentChange {
    # Tell Explorer the environment changed, so new terminals see the Path.
    Add-Type -Namespace Win32 -Name Env -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam,
    string lParam, uint flags, uint timeout, out UIntPtr result);
'@
    $result = [UIntPtr]::Zero
    [void][Win32.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
}

$shell = New-Object -ComObject WScript.Shell
function New-Shortcut($path, $arguments, $description) {
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = Join-Path $target 'orphion.exe'
    $link.Arguments = $arguments
    $link.WorkingDirectory = $target
    $link.Description = $description
    $link.Save()
}
New-Shortcut (Join-Path $startMenu 'Orphion.lnk') '' 'Control your QCY earbuds'

if ($Autostart) {
    New-Shortcut (Join-Path $startup 'Orphion.lnk') '--tray' 'Tray icon for your QCY earbuds'
    # If a tray is already running, this one exits on its own.
    Start-Process (Join-Path $target 'orphion.exe') -ArgumentList '--tray'
}

if ($AddToPath) {
    # Edit the raw registry value: going through [Environment] would expand
    # %VARIABLES% in the user's Path and store it as a plain string.
    $key = Get-Item 'HKCU:\Environment'
    $path = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
    if (($path -split ';') -notcontains $target) {
        $kind = if ($path) { $key.GetValueKind('Path') } else { 'ExpandString' }
        $new = (@($path.TrimEnd(';'), $target) | Where-Object { $_ }) -join ';'
        Set-ItemProperty 'HKCU:\Environment' -Name Path -Value $new -Type $kind
        Send-EnvironmentChange
    }
}

Write-Host "Installed to $target"
Write-Host 'Start "Orphion" from the Start menu.'
