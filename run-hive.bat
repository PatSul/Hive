@echo off
REM ============================================================
REM  Hive - easy launcher. Double-click (or the desktop shortcut)
REM  to run the built app. Prefers the release build; falls back
REM  to the debug build.
REM ============================================================
cd /d "%~dp0hive"

if exist "target\release\hive.exe" (
    echo Launching Hive ^(release^)...
    start "Hive" "target\release\hive.exe"
    goto :eof
)
if exist "target\debug\hive.exe" (
    echo Launching Hive ^(debug^)...
    start "Hive" "target\debug\hive.exe"
    goto :eof
)

echo No built Hive binary was found under hive\target.
echo Build it first from the hive\ folder:
echo     build.bat build --release
echo.
pause
