<#
.SYNOPSIS
    Removes everything install.ps1 created.
.EXAMPLE
    powershell -ExecutionPolicy Bypass -File uninstall.ps1
#>
$ErrorActionPreference = 'Stop'

$target = Join-Path $env:LOCALAPPDATA 'Programs\Orphion'
$startMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Orphion'
$startup = [Environment]::GetFolderPath('Startup')
# Orphion.lnk starts the tray; Orphion tray.lnk is from earlier versions.
$startupLinks = (Join-Path $startup 'Orphion.lnk'), (Join-Path $startup 'Orphion tray.lnk')

# Stop the window and the tray (both orphion), and the separate tray of
# earlier versions. Stop-Process returns before the processes are gone; wait
# for them, then drop our own handles to them (an open process handle keeps
# its .exe locked).
$running = @(Get-Process orphion, orphion-tray -ErrorAction SilentlyContinue)
$running | Stop-Process -Force
$running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
$running | ForEach-Object { $_.Dispose() }

# Windows releases the files of a just-ended program with a short delay.
function Remove-Retrying($item) {
    for ($i = 1; Test-Path $item; $i++) {
        try {
            Remove-Item $item -Recurse -Force
        } catch {
            if ($i -ge 20) { throw }
            Start-Sleep -Milliseconds 250
        }
    }
}

# Program, shortcuts, then settings and the endpoint cache.
$items = $startupLinks + @($startMenu, $target,
    (Join-Path $env:APPDATA 'orphion'), (Join-Path $env:LOCALAPPDATA 'orphion'))
foreach ($item in $items) { Remove-Retrying $item }

# Only touch the user's Path if install.ps1 -AddToPath put us there, and edit
# the raw registry value so %VARIABLES% and the value type are preserved.
$key = Get-Item 'HKCU:\Environment'
$path = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$entries = $path -split ';'
if ($entries -contains $target) {
    $kind = $key.GetValueKind('Path')
    $kept = $entries | Where-Object { $_ -ne $target }
    Set-ItemProperty 'HKCU:\Environment' -Name Path -Value ($kept -join ';') -Type $kind
    # Tell Explorer the environment changed, so new terminals see the Path.
    Add-Type -Namespace Win32 -Name Env -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam,
    string lParam, uint flags, uint timeout, out UIntPtr result);
'@
    $result = [UIntPtr]::Zero
    [void][Win32.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
}

Write-Host 'Orphion has been removed.'
