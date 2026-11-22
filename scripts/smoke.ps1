<#
.SYNOPSIS
Launches wayward on a throwaway config, asserts the bar window appears with the
expected geometry, optionally tries to open the AI popup, then closes the bar and
asserts a clean exit with no windows left behind.

Synthetic mouse input does not reliably reach an iced/winit window from a
non-interactive shell, so the popup check is skipped by default and reported as
SKIP rather than FAIL. Pass -RequirePopup in a context where input injection
works to make it a hard check.

Reports PASS/FAIL/SKIP per check and exits non-zero if any check fails.

.EXAMPLE
./scripts/smoke.ps1
./scripts/smoke.ps1 -Mode real
#>
param(
    [ValidateSet('mock', 'real')]
    [string]$Mode = 'mock',
    [switch]$RequirePopup,
    [int]$PopupSweepSeconds = 20
)

$ErrorActionPreference = 'Stop'
$script:failures = 0

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class SmokeWin32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

function Get-VisibleWindows([uint32]$OwnerPid) {
    $found = New-Object System.Collections.Generic.List[IntPtr]
    $callback = [SmokeWin32+EnumWindowsProc] {
        param($hWnd, $lParam)
        [uint32]$owner = 0
        [void][SmokeWin32]::GetWindowThreadProcessId($hWnd, [ref]$owner)
        if ($owner -eq $OwnerPid -and [SmokeWin32]::IsWindowVisible($hWnd)) {
            $found.Add($hWnd)
        }
        return $true
    }
    [void][SmokeWin32]::EnumWindows($callback, [IntPtr]::Zero)
    return $found
}

function Get-RectOf([IntPtr]$hWnd) {
    $box = New-Object SmokeWin32+RECT
    [void][SmokeWin32]::GetWindowRect($hWnd, [ref]$box)
    return $box
}

function Wait-Until([scriptblock]$Condition, [int]$TimeoutMs) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (& $Condition) { return $true }
        Start-Sleep -Milliseconds 50
    }
    return $false
}

function Find-BarWindow([uint32]$OwnerPid) {
    foreach ($candidate in @(Get-VisibleWindows $OwnerPid)) {
        $box = Get-RectOf $candidate
        $width = $box.Right - $box.Left
        $height = $box.Bottom - $box.Top
        if ($width -ge 500 -and $height -ge 1 -and $height -le 200) {
            return [pscustomobject]@{ Handle = $candidate; Box = $box }
        }
    }
    return $null
}

function Record([string]$Status, [string]$Label) {
    Write-Host "$Status  $Label"
    if ($Status -eq 'FAIL') { $script:failures++ }
}

$binary = @(
    (Join-Path $PSScriptRoot '..\target\debug\wayward-app.exe'),
    'G:\Temp\rust\target\debug\wayward-app.exe'
) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $binary) {
    Write-Host 'FAIL  wayward-app.exe not found; run cargo build -p wayward-app first'
    exit 1
}

$configDir = Join-Path ([System.IO.Path]::GetTempPath()) ('wayward-smoke-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $configDir -Force | Out-Null
$configLines = @('[ai]', 'enabled = true', "mode = `"$Mode`"", 'interval_ms = 300000', 'warn_below = 0.20', 'urgent_below = 0.0')
if ($Mode -eq 'real') {
    $configLines += @('', '[[ai.providers]]', 'id = "claude-code"', 'enabled = true', 'interval_ms = 300000', 'limit = 1000000.0', 'reset_after_secs = 18000')
}
($configLines -join "`n") | Set-Content -LiteralPath (Join-Path $configDir 'wayward.toml') -Encoding utf8

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $binary
$psi.WorkingDirectory = (Split-Path -Parent $PSScriptRoot)
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.Environment['WAYWARD_CONFIG_DIR'] = $configDir
$process = [System.Diagnostics.Process]::Start($psi)
$ownerPid = [uint32]$process.Id

try {
    $bar = $null
    if (Wait-Until { $script:bar = Find-BarWindow $ownerPid; $null -ne $script:bar } 15000) {
        Record 'PASS' 'bar window appeared'
        $bar = $script:bar
        $b = $bar.Box
        Write-Host "      hwnd $($bar.Handle) rect $($b.Left),$($b.Top) - $($b.Right),$($b.Bottom)"
    }
    else {
        Record 'FAIL' 'bar window appeared'
    }

    if ($bar) {
        $known = @([IntPtr[]](Get-VisibleWindows $ownerPid))

        if ($RequirePopup) {
            $popup = [IntPtr]::Zero
            $y = $b.Top + [Math]::Max(4, [int](($b.Bottom - $b.Top) / 2))
            $deadline = [DateTime]::UtcNow.AddSeconds($PopupSweepSeconds)
            while ([DateTime]::UtcNow -lt $deadline -and $popup -eq [IntPtr]::Zero) {
                foreach ($x in ($b.Left + 4..[Math]::Min($b.Right - 4, 700) | Where-Object { $_ % 8 -eq 4 })) {
                    [void][SmokeWin32]::SetCursorPos($x, $y)
                    [SmokeWin32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
                    [SmokeWin32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
                    Start-Sleep -Milliseconds 60
                    foreach ($candidate in @(Get-VisibleWindows $ownerPid)) {
                        if ($candidate -ne $bar.Handle -and $known -notcontains $candidate) { $popup = $candidate; break }
                    }
                    if ($popup -ne [IntPtr]::Zero) { Write-Host "      popup hwnd $popup after click at x=$x"; break }
                    if ([DateTime]::UtcNow -ge $deadline) { break }
                }
            }
            if ($popup -ne [IntPtr]::Zero) {
                Record 'PASS' 'AI popup opened on left-click'
                $p = Get-RectOf $popup
                Record $(if (($p.Right - $p.Left) -gt 0 -and ($p.Bottom - $p.Top) -gt 0) { 'PASS' } else { 'FAIL' }) 'popup has non-zero size'
                [void][SmokeWin32]::PostMessage($popup, 0x0100, [IntPtr]0x1B, [IntPtr]::Zero)
                [void][SmokeWin32]::PostMessage($popup, 0x0101, [IntPtr]0x1B, [IntPtr]::Zero)
                $closed = Wait-Until { (@(Get-VisibleWindows $ownerPid) | Where-Object { $_ -ne $bar.Handle -and $known -notcontains $_ }).Count -eq 0 } 4000
                Record $(if ($closed) { 'PASS' } else { 'FAIL' }) 'Esc closed the popup'
            }
            else {
                Record 'FAIL' 'AI popup opened on left-click'
            }
        }
        else {
            Record 'SKIP' 'AI popup (synthetic input does not reach the iced window here; verify by hand or pass -RequirePopup)'
        }

        $exited = $false
        foreach ($attempt in 1..3) {
            if ($process.HasExited) { $exited = $true; break }
            [void][SmokeWin32]::PostMessage($bar.Handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
            if (Wait-Until { $process.HasExited } 3000) { $exited = $true; break }
        }
        Record $(if ($exited) { 'PASS' } else { 'FAIL' }) 'process exited on window close'
        if ($exited) {
            $left = (Get-VisibleWindows $ownerPid).Count
            Record $(if ($left -eq 0) { 'PASS' } else { 'FAIL' }) "no windows left behind ($left)"
        }
    }

    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $aiLine = ($stdout -split "`n") | Where-Object { $_ -match 'AI module enabled' } | Select-Object -First 1
    if ($aiLine) { Write-Host "      log: $($aiLine.Trim())" }
    if ($stderr -match 'panic') { Write-Host "      stderr: $stderr"; Record 'FAIL' 'no panic on stderr' }
}
catch {
    Write-Host "FAIL  $($_.Exception.Message)"
    Write-Host $_.ScriptStackTrace
    $script:failures++
}
finally {
    if (-not $process.HasExited) {
        [void][SmokeWin32]::PostMessage($bar.Handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 500
    }
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    Remove-Item -LiteralPath $configDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ''
if ($script:failures -eq 0) { Write-Host 'smoke PASS'; exit 0 }
Write-Host "smoke FAIL ($($script:failures) failing checks)"; exit 1