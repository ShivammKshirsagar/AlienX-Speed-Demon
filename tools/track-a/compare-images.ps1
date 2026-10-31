param(
    [string]$Baseline = "speeddemon-check/track-a-baseline",
    [string]$Candidate = "speeddemon-check/track-a",
    [string]$Data = ".\data\a",
    [string]$Output = ".\.out\track-a",
    [int]$Pairs = 5
)

$dataPath = (Resolve-Path -LiteralPath $Data).Path
$outputPath = (Resolve-Path -LiteralPath $Output).Path
$expected = "495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3"
$baselineTimes = [System.Collections.Generic.List[double]]::new()
$candidateTimes = [System.Collections.Generic.List[double]]::new()
$baselineMemory = [System.Collections.Generic.List[double]]::new()
$candidateMemory = [System.Collections.Generic.List[double]]::new()

function Invoke-MeasuredImage([string]$Image) {
    $dockerArgs = @(
        "run", "--rm", "--network=none", "--read-only", "--tmpfs", "/tmp:rw,size=1g,exec",
        "--memory=8g", "--memory-swap=8g", "--pids-limit=512", "--cap-drop=ALL",
        "--security-opt=no-new-privileges", "--user", "65534:65534", "--ulimit", "nofile=8192:8192",
        "--cpuset-cpus=0-11", "-v", "${dataPath}:/data:ro", "-v", "${outputPath}:/out:rw",
        "--entrypoint", "/out/time-wrapper", $Image
    )
    $lines = @(& docker @dockerArgs 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "Docker run failed for ${Image}: $($lines -join ' ')" }
    $timeLine = $lines | Where-Object { $_ -match '^process_ns=\d+$' } | Select-Object -Last 1
    $memoryLine = $lines | Where-Object { $_ -match '^memory_peak_bytes=\d+$' } | Select-Object -Last 1
    if (-not $timeLine -or -not $memoryLine) { throw "Missing metrics for ${Image}: $($lines -join ' ')" }
    $hash = (Get-FileHash -LiteralPath (Join-Path $outputPath "summary.txt") -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $expected) { throw "Hash mismatch for ${Image}: $hash" }
    return [pscustomobject]@{
        Seconds = [double](($timeLine -split '=')[1]) / 1e9
        MemoryMb = [double](($memoryLine -split '=')[1]) / 1MB
    }
}

for ($i = 1; $i -le $Pairs; $i++) {
    if ($i % 2) {
        $b = Invoke-MeasuredImage $Baseline
        $c = Invoke-MeasuredImage $Candidate
    } else {
        $c = Invoke-MeasuredImage $Candidate
        $b = Invoke-MeasuredImage $Baseline
    }
    $baselineTimes.Add($b.Seconds); $baselineMemory.Add($b.MemoryMb)
    $candidateTimes.Add($c.Seconds); $candidateMemory.Add($c.MemoryMb)
    Write-Output ("pair {0}: baseline {1:N2}s/{2:N1}MB candidate {3:N2}s/{4:N1}MB" -f $i, $b.Seconds, $b.MemoryMb, $c.Seconds, $c.MemoryMb)
}

function Median([System.Collections.Generic.List[double]]$Values) {
    $sorted = @($Values | Sort-Object)
    return $sorted[[int][Math]::Floor($sorted.Count / 2)]
}

Write-Output ("medians: baseline {0:N2}s/{1:N1}MB candidate {2:N2}s/{3:N1}MB" -f (Median $baselineTimes), (Median $baselineMemory), (Median $candidateTimes), (Median $candidateMemory))
