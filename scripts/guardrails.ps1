$ErrorActionPreference = 'Stop'

$failures = New-Object System.Collections.Generic.List[string]

function Add-Failure([string]$check, [string]$standard, [string[]]$hits) {
    $failures.Add("$check`n  standard: $standard`n  offending: $($hits -join '; ')")
}

$viewFiles = @('crates/wayward-app/src/view.rs', 'crates/wayward-app/src/popup_view.rs')

$colorHits = @(Select-String -Path $viewFiles -Pattern '#[0-9a-fA-F]{6}\b' -AllMatches |
    ForEach-Object { "$($_.Filename):$($_.LineNumber) $($_.Line.Trim())" })
if ($colorHits.Count -gt 0) {
    Add-Failure 'hardcoded color in a view' 'AGENTS.md 5: colors come only from the theme; add a named theme key instead' $colorHits
}

foreach ($manifest in @('crates/wayward-core/Cargo.toml', 'crates/wayward-ai/Cargo.toml')) {
    $depHits = @(Select-String -Path $manifest -Pattern '^\s*(iced|windows)\s*=' |
        ForEach-Object { "$($_.Filename):$($_.LineNumber) $($_.Line.Trim())" })
    if ($depHits.Count -gt 0) {
        Add-Failure "forbidden dependency in $manifest" 'AGENTS.md 3: core and ai stay free of iced and windows; put Win32 behind wayward-platform' $depHits
    }
}

$quotaHits = @(Select-String -Path (Get-ChildItem -Path 'config' -Filter '*.toml' -Recurse).FullName `
    -Pattern 'quota' -CaseSensitive:$false -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -notlike '*\themes\*' } |
    ForEach-Object { "$($_.Filename):$($_.LineNumber) $($_.Line.Trim())" })
$quotaHits += @(Select-String -Path 'crates/*/src/*.rs' -Pattern 'quota' -CaseSensitive:$false -ErrorAction SilentlyContinue |
    ForEach-Object { "$($_.Filename):$($_.LineNumber) $($_.Line.Trim())" })
if ($quotaHits.Count -gt 0) {
    Add-Failure 'vendor term "quota" outside the glossary' 'GLOSSARY.md: local providers report a usage window; quota is vendor-reported only' $quotaHits
}

$tokioLine = @(Select-String -LiteralPath 'Cargo.toml' -Pattern '^tokio\s*=' |
    ForEach-Object { $_.Line.Trim() })
$devOnly = @($tokioLine | Where-Object { $_ -match 'macros|test-util' })
if ($devOnly.Count -gt 0) {
    Add-Failure 'dev-only tokio features in [workspace.dependencies]' 'dev features belong in the consuming crate dev-dependencies: workspace tokio is time + rt only' $devOnly
}

if ($failures.Count -gt 0) {
    Write-Output 'guardrails FAILED:'
    foreach ($failure in $failures) {
        Write-Output "  - $failure"
    }
    exit 1
}

Write-Output 'guardrails ok: no hardcoded view colors, no iced/windows in core or ai, no vendor quota term, no dev-only tokio features.'