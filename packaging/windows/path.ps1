<#
.SYNOPSIS
    Adds a folder to, or removes it from, the user or system Path.

.DESCRIPTION
    Used by the Orphion setup program. NSIS strings are limited to 1024
    characters, which a long Path can exceed, so the Path is edited here.
    The raw registry value is edited: going through [Environment] would expand
    %VARIABLES% and store the value as a plain string. The caller broadcasts
    the environment change.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File path.ps1 -Scope User -Action Add -Dir "C:\Apps\Orphion"
#>
param(
    [Parameter(Mandatory)] [ValidateSet('User', 'Machine')] [string]$Scope,
    [Parameter(Mandatory)] [ValidateSet('Add', 'Remove')] [string]$Action,
    [Parameter(Mandatory)] [string]$Dir
)
$ErrorActionPreference = 'Stop'

$keyPath = if ($Scope -eq 'User') {
    'HKCU:\Environment'
} else {
    'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment'
}
$key = Get-Item $keyPath
$path = $key.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$kind = if ($path) { $key.GetValueKind('Path') } else { 'ExpandString' }
$entries = @($path -split ';' | Where-Object { $_ })
$dir = $Dir.TrimEnd('\')

if ($Action -eq 'Add') {
    if ($entries | Where-Object { $_.TrimEnd('\') -eq $dir }) { exit 0 }
    $new = ($entries + $dir) -join ';'
} else {
    $kept = @($entries | Where-Object { $_.TrimEnd('\') -ne $dir })
    if ($kept.Count -eq $entries.Count) { exit 0 }
    $new = $kept -join ';'
}
Set-ItemProperty $keyPath -Name Path -Value $new -Type $kind
