#requires -Version 5.1
<#
.SYNOPSIS
    Analyzes the Zap BYOP prompt cache hit rate (based on the `[byop-cache]`
    log line printed at the end of every stream by chat_stream.rs::generate_byop_output).

.DESCRIPTION
    1. Automatically locates the Zap log file: `%LOCALAPPDATA%\zap\Zap\data\logs\zap.log`
    2. Greps lines shaped like the following:
       [byop-cache] prompt_tokens=N cache_read=R (X.X%) cache_create=W (Y.Y%) model=M compaction=L
       where compaction= is an optional field added by P2-16 (none / inactive / active(hidden=N))
    3. Aggregates by model group and outputs, per model:
       - Request count
       - Average cache_read ratio (the main hit-rate metric)
       - Average cache_create ratio (write metric; high on the first request, should drop afterward)
       - Total prompt tokens / total cache_read tokens / total cache_create tokens
       - Compaction-related request stats (P2-16)
    4. Provides a "comparison mode" (-Tail N) that only looks at the most recent N records, useful for A/B comparisons

.PARAMETER LogPath
    A custom log path. Defaults to looking it up from Zap's standard locations.

.PARAMETER Tail
    Only analyze the most recent N `[byop-cache]` lines (default: all).

.PARAMETER Watch
    Continuously tail the log, printing new hit-rate lines in real time (Ctrl+C to exit).

.EXAMPLE
    .\analyze-prompt-cache.ps1
.EXAMPLE
    .\analyze-prompt-cache.ps1 -Tail 20
.EXAMPLE
    .\analyze-prompt-cache.ps1 -Watch
.EXAMPLE
    .\analyze-prompt-cache.ps1 -LogPath "D:\backup\zap.log"

.NOTES
    Requires Zap to have INFO-level logging enabled (`[byop-cache]` is a log::info!).
    If there are no `[byop-cache]` lines at all:
      - The upstream provider didn't return a cache field (DeepSeek/Ollama's implicit cache may just be 0)
      - Or RUST_LOG filtered out INFO
#>
[CmdletBinding()]
param(
    [string]$LogPath,
    [int]$Tail = 0,
    [switch]$Watch
)

$ErrorActionPreference = 'Stop'

# ---------- 1. Locate the log ----------
function Resolve-ZapLog {
    param([string]$Override)
    if ($Override) {
        if (-not (Test-Path -LiteralPath $Override)) {
            throw "The specified log path does not exist: $Override"
        }
        return (Resolve-Path -LiteralPath $Override).Path
    }
    $candidates = @()
    if ($env:LOCALAPPDATA) {
        # Current version's path (the Windows branch of `crates/simple_logger/src/manager.rs::log_directory_path`)
        $candidates += (Join-Path -Path $env:LOCALAPPDATA -ChildPath 'zap\Zap\data\logs\zap.log')
        # Fallbacks (paths used by older versions)
        $candidates += (Join-Path -Path $env:LOCALAPPDATA -ChildPath 'zap\Zap\data\zap.log')
        $candidates += (Join-Path -Path $env:LOCALAPPDATA -ChildPath 'zap\Zap\zap.log')
    }
    if ($env:APPDATA) {
        $candidates += (Join-Path -Path $env:APPDATA -ChildPath 'zap\Zap\data\logs\zap.log')
        $candidates += (Join-Path -Path $env:APPDATA -ChildPath 'zap\Zap\data\zap.log')
    }
    foreach ($c in $candidates) {
        if ($c -and (Test-Path -LiteralPath $c)) { return (Resolve-Path -LiteralPath $c).Path }
    }
    throw @"
Zap log file not found. Please check the following locations, or specify one explicitly with -LogPath:
  $($candidates -join "`n  ")
If Zap hasn't been run yet, start it once first before running this script.
"@
}

# ---------- 2. Parse a single line ----------
# Line format (single line, may be wrapped due to terminal width, but the log
# crate's own line breaks only occur at the end):
# [byop-cache] prompt_tokens=12345 cache_read=10000 (81.0%) cache_create=200 (1.6%) model=claude-opus-4-7 compaction=none
# The compaction= field was added by P2-16, with values: none / inactive / active(hidden=N).
# For compatibility with older logs, the compaction field is optional.
$cacheLineRegex = [regex]'\[byop-cache\]\s+prompt_tokens=(?<prompt>\d+)\s+cache_read=(?<read>\d+)\s+\(\s*(?<read_pct>[\d\.]+)%\)\s+cache_create=(?<create>\d+)\s+\(\s*(?<create_pct>[\d\.]+)%\)\s+model=(?<model>\S+?)(?:\s+compaction=(?<compaction>\S+))?$'

function Parse-CacheLine {
    param([string]$Line)
    $m = $cacheLineRegex.Match($Line)
    if (-not $m.Success) { return $null }
    $compactionRaw = if ($m.Groups['compaction'].Success) { $m.Groups['compaction'].Value } else { '' }
    [pscustomobject]@{
        Timestamp    = $null
        PromptTokens = [int]$m.Groups['prompt'].Value
        CacheRead    = [int]$m.Groups['read'].Value
        CacheCreate  = [int]$m.Groups['create'].Value
        ReadPct      = [double]$m.Groups['read_pct'].Value
        CreatePct    = [double]$m.Groups['create_pct'].Value
        Model        = $m.Groups['model'].Value
        # P2-16: compaction status. Values: '' (old logs) / 'none' / 'inactive' / 'active(hidden=N)'
        Compaction   = $compactionRaw
        Raw          = $Line
    }
}

# ---------- 3. Aggregate output ----------
function Format-Summary {
    param([System.Collections.IList]$Records)
    if ($Records.Count -eq 0) {
        Write-Host 'No [byop-cache] lines matched.' -ForegroundColor Yellow
        Write-Host @'

Possible reasons:
  1. No request has gone through the BYOP path yet (no AI conversation happened since Zap started)
  2. The upstream provider didn't return a cache field (DeepSeek/Ollama server-side implicit cache)
  3. RUST_LOG filtered out INFO-level logging - check the startup environment variable

Troubleshooting steps:
  $env:RUST_LOG = 'info'   # set before starting Zap
  Send 2 messages to the AI in Zap (same conversation), to trigger BYOP
  Then rerun this script
'@ -ForegroundColor Yellow
        return
    }

    Write-Host ''
    Write-Host '========== Zap BYOP Prompt Cache Hit Rate Analysis ==========' -ForegroundColor Cyan
    Write-Host ("Total matched lines: {0}" -f $Records.Count)

    # P2-16: compaction-related summary
    $compactionActive = @($Records | Where-Object { $_.Compaction -like 'active*' })
    if ($compactionActive.Count -gt 0) {
        Write-Host ("  `-- of which went through the compaction path: {0}" -f $compactionActive.Count) -ForegroundColor DarkYellow
    }
    Write-Host ''

    # Group by model
    $byModel = $Records | Group-Object Model

    $byModel | ForEach-Object {
        $model = $_.Name
        $rs    = $_.Group
        $n     = $rs.Count
        $sumPrompt = ($rs | Measure-Object PromptTokens -Sum).Sum
        $sumRead   = ($rs | Measure-Object CacheRead    -Sum).Sum
        $sumCreate = ($rs | Measure-Object CacheCreate  -Sum).Sum
        $avgReadPct   = ($rs | Measure-Object ReadPct   -Average).Average
        $avgCreatePct = ($rs | Measure-Object CreatePct -Average).Average

        $globalReadPct = if ($sumPrompt -gt 0) { 100.0 * $sumRead / $sumPrompt } else { 0.0 }
        $globalCreatePct = if ($sumPrompt -gt 0) { 100.0 * $sumCreate / $sumPrompt } else { 0.0 }

        Write-Host ("Model: {0}" -f $model) -ForegroundColor Green
        Write-Host ("  Requests:            {0}" -f $n)
        Write-Host ("  Total prompt tokens: {0:N0}" -f $sumPrompt)
        Write-Host ("  Total cache_read:    {0:N0}  ({1:F1}% of total)" -f $sumRead,   $globalReadPct)
        Write-Host ("  Total cache_create:  {0:N0}  ({1:F1}% of total)" -f $sumCreate, $globalCreatePct)
        Write-Host ("  Average read ratio:  {0:F1}%   <- main hit-rate metric (>=20% is normal, >=50% is excellent)" -f $avgReadPct)
        Write-Host ("  Average create ratio:{0:F1}%   <- should drop as turns progress" -f $avgCreatePct)

        # Trend analysis (turn number vs read ratio): see whether the hit rate rises as the conversation progresses
        if ($n -ge 3) {
            $trend = $rs | ForEach-Object -Begin { $i = 0 } -Process {
                $i++
                $marker = if ($_.Compaction -like 'active*') { '*' } else { '' }
                "{0}{1}:{2:F0}%" -f $i, $marker, $_.ReadPct
            }
            Write-Host ("  Read ratio trend:  {0}" -f ($trend -join ' -> '))
            if ($rs | Where-Object { $_.Compaction -like 'active*' }) {
                Write-Host ("  (* = went through the compaction path, a cache miss on that turn is expected)") -ForegroundColor DarkGray
            }
        }
        Write-Host ''
    }

    # Overall health verdict
    $allReadPct = ($Records | Measure-Object ReadPct -Average).Average
    Write-Host '----------- Overall health -----------' -ForegroundColor Cyan
    if ($allReadPct -ge 50) {
        Write-Host ("✅ Overall average hit rate {0:F1}% - excellent" -f $allReadPct) -ForegroundColor Green
    } elseif ($allReadPct -ge 20) {
        Write-Host ("⚠️  Overall average hit rate {0:F1}% - normal, but there's room for improvement" -f $allReadPct) -ForegroundColor Yellow
    } else {
        Write-Host ("❌ Overall average hit rate {0:F1}% - low, there may be a prefix-instability problem" -f $allReadPct) -ForegroundColor Red
        Write-Host '   Where to look: check whether the system prompt has any field that changes per request, and whether MCP tools order is stable'
    }

    if ($compactionActive.Count -gt 0) {
        $nonCompactionRecords = @($Records | Where-Object { $_.Compaction -notlike 'active*' })
        if ($nonCompactionRecords.Count -gt 0) {
            $nonCompactionAvg = ($nonCompactionRecords | Measure-Object ReadPct -Average).Average
            Write-Host ("ℹ️  Average hit rate excluding compaction turns: {0:F1}% (n={1})" -f $nonCompactionAvg, $nonCompactionRecords.Count) -ForegroundColor DarkCyan
        }
    }
}

# ---------- 4. Main flow ----------
$logFile = Resolve-ZapLog -Override $LogPath
Write-Host "Log path: $logFile" -ForegroundColor DarkGray

if ($Watch) {
    Write-Host 'Entering watch mode, Ctrl+C to exit. New [byop-cache] lines will be printed in real time:' -ForegroundColor Cyan
    Get-Content -LiteralPath $logFile -Wait -Tail 0 | ForEach-Object {
        $rec = Parse-CacheLine $_
        if ($rec) {
            $color = if ($rec.ReadPct -ge 50) { 'Green' }
                     elseif ($rec.ReadPct -ge 20) { 'Yellow' }
                     else { 'Red' }
            $compactionTag = if ($rec.Compaction) { " [$($rec.Compaction)]" } else { '' }
            $msg = '[{0}] read={1:F1}% create={2:F1}% prompt={3} model={4}{5}' -f `
                (Get-Date -Format 'HH:mm:ss'), $rec.ReadPct, $rec.CreatePct, $rec.PromptTokens, $rec.Model, $compactionTag
            Write-Host $msg -ForegroundColor $color
        }
    }
    return
}

# Static analysis (one-time scan)
$records = New-Object System.Collections.ArrayList
Get-Content -LiteralPath $logFile -ReadCount 1000 | ForEach-Object {
    foreach ($line in $_) {
        $rec = Parse-CacheLine $line
        if ($rec) { [void]$records.Add($rec) }
    }
}

if ($Tail -gt 0 -and $records.Count -gt $Tail) {
    $records = [System.Collections.ArrayList]::new(
        $records.GetRange($records.Count - $Tail, $Tail)
    )
    Write-Host "(only counting the most recent $Tail entries)" -ForegroundColor DarkGray
}

Format-Summary -Records $records
