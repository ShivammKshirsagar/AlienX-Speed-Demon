param(
    [string]$Harness = "C:\GitHub Desktop\speed-demon-2026",
    [int[]]$Seeds = @(2, 3, 7)
)

$generator = Join-Path $Harness "generators\a_ingest.py"
$reference = Join-Path $Harness "reference\a_ingest.py"
foreach ($seed in $Seeds) {
    $data = ".\tools\track-a\data\seed$seed"
    $referenceOut = ".\tools\track-a\out\reference$seed"
    New-Item -ItemType Directory -Force -Path $data, $referenceOut | Out-Null
    & python $generator --seed $seed --scale sample --out $data
    if ($LASTEXITCODE -ne 0) { throw "Generator failed for seed $seed" }
    & python $reference --data $data --out $referenceOut
    if ($LASTEXITCODE -ne 0) { throw "Reference failed for seed $seed" }
    $expected = (Get-FileHash -LiteralPath (Join-Path $referenceOut "summary.txt") -Algorithm SHA256).Hash.ToLowerInvariant()
    & python .\check.py a --skip-build --data $data --expected $expected
    if ($LASTEXITCODE -ne 0) { throw "Linux check failed for seed $seed" }
    Write-Output "seed $seed sample: $expected"
}
