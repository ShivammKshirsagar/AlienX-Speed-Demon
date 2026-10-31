param(
    [string]$Image = "speeddemon-check/track-a",
    [string]$Data = ".\data\a",
    [string]$Output = ".\.out\track-a",
    [int]$Repeats = 5,
    [int]$Threads = 12,
    [int]$ChunkSize = 1048576,
    [string]$Advice = "",
    [switch]$MapPopulate,
    [switch]$Pread
)

$dataPath = (Resolve-Path -LiteralPath $Data).Path
$outputPath = (Resolve-Path -LiteralPath $Output).Path
$expected = "495ca09682b9673ef836a506496177680bb4a3af54255174befddb1f83481cd3"
for ($i = 1; $i -le $Repeats; $i++) {
    $dockerArgs = @(
        "run", "--rm", "--network=none", "--read-only", "--tmpfs", "/tmp:rw,size=1g,exec",
        "--memory=8g", "--memory-swap=8g", "--pids-limit=512", "--cap-drop=ALL",
        "--security-opt=no-new-privileges", "--user", "65534:65534", "--ulimit", "nofile=8192:8192",
        "--cpuset-cpus=0-11", "-e", "PROFILE_THREADS=$Threads", "-e", "PROFILE_CHUNK_SIZE=$ChunkSize", "-v", "${dataPath}:/data:ro", "-v", "${outputPath}:/out:rw",
        "--entrypoint", "/out/profile-solution", $Image
    )
    if ($Advice) {
        $dockerArgs = $dockerArgs[0..($dockerArgs.Length - 2)] + @("-e", "PROFILE_ADVICE=$Advice", $dockerArgs[-1])
    }
    if ($MapPopulate) {
        $dockerArgs = $dockerArgs[0..($dockerArgs.Length - 2)] + @("-e", "PROFILE_MAP_POPULATE=1", $dockerArgs[-1])
    }
    if ($Pread) {
        $dockerArgs = $dockerArgs[0..($dockerArgs.Length - 2)] + @("-e", "PROFILE_PREAD=1", $dockerArgs[-1])
    }
    $lines = @(& docker @dockerArgs 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "Profiler run $i failed: $($lines -join ' ')" }
    $hash = (Get-FileHash -LiteralPath (Join-Path $outputPath "summary.txt") -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $expected) { throw "Hash mismatch on profiler run ${i}: $hash" }
    Write-Output "run $i hash=$hash"
    $lines | Where-Object { $_ -like 'profile *' }
}
