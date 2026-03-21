$vsPath = "C:\Program Files\Microsoft Visual Studio\18\Enterprise\VC\Auxiliary\Build\vcvars64.bat"

# Import VS environment variables
$output = cmd /c "`"$vsPath`" >nul 2>&1 && set" 2>&1
foreach ($line in $output) {
    if ($line -match "^([^=]+)=(.*)$") {
        [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
}

Write-Host "CL.EXE: $(Get-Command cl.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source)"
Set-Location "H:\WORK\AG\AIrglowStudio\hive"
Write-Host "=== STARTING CARGO CHECK ==="
& cargo check 2>&1 | ForEach-Object { Write-Host $_ }
Write-Host "=== EXIT CODE: $LASTEXITCODE ==="
