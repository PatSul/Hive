@echo off
call "C:\Program Files\Microsoft Visual Studio\18\Enterprise\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
if errorlevel 1 (
    echo VCVARS FAILED
    exit /b 1
)
echo VCVARS OK
where cl.exe
cd /d H:\WORK\AG\AIrglowStudio\hive
echo === STARTING CARGO CHECK ===
cargo check 2>&1
echo === CARGO CHECK EXIT CODE: %ERRORLEVEL% ===
