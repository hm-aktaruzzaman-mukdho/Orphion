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

# Running copies would keep the files locked. Stop-Process returns before
# they are gone: wait for them, then drop our own handles to them (an open
# process handle keeps its .exe locked).
$running = @(Get-Process qcy-gui, qcy-tray -ErrorAction SilentlyContinue)
$running | Stop-Process -Force
$running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
$running | ForEach-Object { $_.Dispose() }

New-Item -ItemType Directory -Force -Path $target, $startMenu | Out-Null
foreach ($file in 'qcy-gui.exe', 'qcy-tray.exe', 'qcyctl.exe', 'README.txt') {
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
Write-Host 'Start "QCY Earbuds" from the Start menu.'
