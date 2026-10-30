param(
    [string]$Solution = ".\track-a\target\release\solution.exe",
    [string]$Data = ".\data\a\requests.log",
    [int]$Repeats = 5
)

$solutionPath = (Resolve-Path -LiteralPath $Solution).Path
$dataPath = (Resolve-Path -LiteralPath $Data).Path
$outputPath = Join-Path (Resolve-Path -LiteralPath ".\tools\track-a").Path "bench-summary.txt"
$referencePath = (Resolve-Path -LiteralPath "C:\GitHub Desktop\speed-demon-2026\reference\a_ingest.py").Path
$referenceOut = Join-Path (Resolve-Path -LiteralPath ".\tools\track-a").Path "bench-reference"
$null = New-Item -ItemType Directory -Force -Path $referenceOut
$times = [System.Collections.Generic.List[double]]::new()
$peaks = [System.Collections.Generic.List[double]]::new()

$baseline = [System.Diagnostics.Process]::new()
$baseline.StartInfo.FileName = "python"
$baseline.StartInfo.UseShellExecute = $false
$baseline.StartInfo.Arguments = '"' + $referencePath + '" --data "' + (Split-Path -Parent $dataPath) + '" --out "' + $referenceOut + '"'
$baselineWatch = [System.Diagnostics.Stopwatch]::StartNew()
[void]$baseline.Start()
$baselinePeak = 0L
while (-not $baseline.HasExited) {
    $baseline.Refresh()
    if ($baseline.WorkingSet64 -gt $baselinePeak) { $baselinePeak = $baseline.WorkingSet64 }
    Start-Sleep -Milliseconds 20
}
$baseline.Refresh()
if ($baseline.PeakWorkingSet64 -gt $baselinePeak) { $baselinePeak = $baseline.PeakWorkingSet64 }
$baselineWatch.Stop()
if ($baseline.ExitCode -ne 0) { throw "Reference exited $($baseline.ExitCode)" }
$baselineHash = (Get-FileHash -LiteralPath (Join-Path $referenceOut "summary.txt") -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output ("reference: {0:N3} s, {1:N1} MB peak working set, {2}" -f $baselineWatch.Elapsed.TotalSeconds, ($baselinePeak / 1MB), $baselineHash)

for ($i = 1; $i -le $Repeats; $i++) {
    $proc = [System.Diagnostics.Process]::new()
    $proc.StartInfo.FileName = $solutionPath
    $proc.StartInfo.UseShellExecute = $false
    $proc.StartInfo.Arguments = '"' + $dataPath + '" "' + $outputPath + '"'
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    [void]$proc.Start()
    $peak = 0L
    while (-not $proc.HasExited) {
        $proc.Refresh()
        if ($proc.WorkingSet64 -gt $peak) { $peak = $proc.WorkingSet64 }
        Start-Sleep -Milliseconds 10
    }
    $proc.Refresh()
    if ($proc.PeakWorkingSet64 -gt $peak) { $peak = $proc.PeakWorkingSet64 }
    $watch.Stop()
    if ($proc.ExitCode -ne 0) { throw "Run $i exited $($proc.ExitCode)" }
    $times.Add($watch.Elapsed.TotalSeconds)
    $peaks.Add($peak / 1MB)
    $hash = (Get-FileHash -LiteralPath $outputPath -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Output ("run {0}: {1:N3} s, {2:N1} MB peak working set, {3}" -f $i, $watch.Elapsed.TotalSeconds, ($peak / 1MB), $hash)
}

$sortedTimes = @($times | Sort-Object)
$sortedPeaks = @($peaks | Sort-Object)
Write-Output ("median: {0:N3} s, {1:N1} MB peak working set" -f $sortedTimes[[int]($Repeats / 2)], $sortedPeaks[[int]($Repeats / 2)])
