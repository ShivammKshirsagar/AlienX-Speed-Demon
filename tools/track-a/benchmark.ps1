param(
    [string]$Image = "speeddemon-check/track-a",
    [string]$Data = ".\data\a",
    [string]$Output = ".\.out\track-a",
    [int]$Repeats = 5
)

$dataPath = (Resolve-Path -LiteralPath $Data).Path
$outputPath = (Resolve-Path -LiteralPath $Output).Path
$expected = "495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3"
$times = [System.Collections.Generic.List[double]]::new()
$memories = [System.Collections.Generic.List[double]]::new()

for ($i = 1; $i -le $Repeats; $i++) {
    $dockerArgs = @(
        "run", "--rm", "--network=none", "--read-only",
        "--tmpfs", "/tmp:rw,size=1g,exec", "--memory=8g", "--memory-swap=8g",
        "--pids-limit=512", "--cap-drop=ALL", "--security-opt=no-new-privileges",
        "--user", "65534:65534", "--ulimit", "nofile=8192:8192", "--cpuset-cpus=0-11",
        "-v", "${dataPath}:/data:ro", "-v", "${outputPath}:/out:rw",
        "--entrypoint", "/out/time-wrapper", $Image
    )
    $lines = @(& docker @dockerArgs 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "Docker run $i failed: $($lines -join ' ')" }

    $timeLine = $lines | Where-Object { $_ -match '^process_ns=\d+$' } | Select-Object -Last 1
    $memoryLine = $lines | Where-Object { $_ -match '^memory_peak_bytes=\d+$' } | Select-Object -Last 1
    if (-not $timeLine -or -not $memoryLine) { throw "Could not parse timing/memory: $($lines -join ' ')" }
    $seconds = [double](($timeLine -split '=')[1]) / 1e9
    $memoryMb = [double](($memoryLine -split '=')[1]) / 1MB
    $hash = (Get-FileHash -LiteralPath (Join-Path $outputPath "summary.txt") -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $expected) { throw "Hash mismatch on run ${i}: $hash" }
    $times.Add($seconds)
    $memories.Add($memoryMb)
    Write-Output ("run {0}: {1:N2} s process, {2:N1} MB cgroup peak, {3}" -f $i, $seconds, $memoryMb, $hash)
}

$sortedTimes = @($times | Sort-Object)
$sortedMemories = @($memories | Sort-Object)
$middle = [int][Math]::Floor($Repeats / 2)
Write-Output ("median: {0:N2} s process, {1:N1} MB cgroup peak" -f $sortedTimes[$middle], $sortedMemories[$middle])
