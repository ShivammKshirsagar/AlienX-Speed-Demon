param(
    [string]$ControlImage = 'local/track-a-control-bench',
    [string]$CandidateImage = 'local/track-a-h1-bench',
    [string]$DataVolume = 'track-a-full-20261101',
    [string]$OutputVolume = 'track-a-results-20261101',
    [int]$Runs = 15,
    [string]$Name = 'h1',
    [string]$CandidateRoot = 'tools/track-a/work/two-pass'
)

$rows = [System.Collections.Generic.List[object]]::new()
for ($run = 1; $run -le $Runs; $run++) {
    $order = if ($run % 2) { @('control', 'candidate') } else { @('candidate', 'control') }
    foreach ($kind in $order) {
        $image = if ($kind -eq 'control') { $ControlImage } else { $CandidateImage }
        $args = @(
            'run', '--rm', '--cpuset-cpus=0-11', '--memory=8g', '--memory-swap=8g',
            '--user', '65534:65534', '--read-only', '--tmpfs', '/tmp:rw,size=1g,exec',
            '-v', "${DataVolume}:/data:ro", '-v', "${OutputVolume}:/out:rw",
            $image, '/data/requests.log', '/out/summary.txt'
        )
        $raw = (& docker @args 2>&1 | Out-String).Trim()
        if ($LASTEXITCODE -ne 0) { throw "$kind run $run failed: $raw" }
        $match = [regex]::Match($raw, 'VOLUME_METRICS wall_ns=(\d+) cpu_usec=(\d+) vmhwm_kb=(\d+) memory_peak_bytes=(\d+)')
        if (-not $match.Success) { throw "Missing metrics for $kind run $run`: $raw" }
        $rows.Add([pscustomobject]@{
            variant = $kind; run = $run
            wall_ms = [double]$match.Groups[1].Value / 1e6
            cpu_ms = [double]$match.Groups[2].Value / 1e3
            vmhwm_kb = [int64]$match.Groups[3].Value
            memory_peak_bytes = [int64]$match.Groups[4].Value
        })
        Write-Output ("{0} {1}: wall={2:N3}ms cpu={3:N3}ms VmHWM={4}KB cgroup_peak={5}B" -f $kind, $run, $rows[-1].wall_ms, $rows[-1].cpu_ms, $rows[-1].vmhwm_kb, $rows[-1].memory_peak_bytes)
    }
}

$path = "tools/track-a/out/$Name-volume.tsv"
$rows | Export-Csv -Delimiter "`t" -NoTypeInformation -Path $path
$inputKb = 238836234 / 1024.0
foreach ($kind in @('control', 'candidate')) {
    $values = @($rows | Where-Object variant -eq $kind)
    $sorted = @($values | Sort-Object wall_ms)
    $medianWall = ($values | Measure-Object wall_ms -Average).Average
    $wallMedian = ($sorted[7].wall_ms)
    $cpuSorted = @($values | Sort-Object cpu_ms)
    $medCpu = $cpuSorted[7].cpu_ms
    $maxRss = ($values | Measure-Object vmhwm_kb -Maximum).Maximum
    $proxy = $maxRss - $inputKb
    $minRun = $sorted[0]
    Write-Output ("SUMMARY {0}: min_wall={1:N3}ms median_wall={2:N3}ms median_cpu={3:N3}ms cpu_at_min_wall={4:N3}ms max_VmHWM={5}KB max_RSS_proxy={6:N1}KB" -f $kind, $minRun.wall_ms, $wallMedian, $medCpu, $minRun.cpu_ms, $maxRss, $proxy)
}
$control = @($rows | Where-Object variant -eq 'control' | Sort-Object wall_ms)[0]
$candidate = @($rows | Where-Object variant -eq 'candidate' | Sort-Object wall_ms)[0]
$controlProxy = (@($rows | Where-Object variant -eq 'control') | Measure-Object vmhwm_kb -Maximum).Maximum - $inputKb
$candidateProxy = (@($rows | Where-Object variant -eq 'candidate') | Measure-Object vmhwm_kb -Maximum).Maximum - $inputKb
$tPred = 0.066 * [math]::Sqrt(($candidate.cpu_ms / $control.cpu_ms) * ($candidate.wall_ms / $control.wall_ms))
$mPred = 47 + ($candidateProxy - $controlProxy) / 1000
$s = (python tools/track-a/count-lines $CandidateRoot | Select-String -Pattern 'S=(\d+)' | ForEach-Object { [int]$_.Matches[0].Groups[1].Value })
$kPred = 21.852 * [math]::Sqrt(0.066 / $tPred) * [math]::Pow(47 / $mPred, 0.2) * [math]::Pow(497 / $s, 0.15)
Write-Output ("PREDICTED T={0:N4}s M={1:N3}MB S={2} K={3:N3} points={4:N3}" -f $tPred, $mPred, $s, $kPred, (50 * [math]::Log10($kPred)))
Write-Output "raw measurements: $path"
