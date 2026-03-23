@echo off
setlocal

echo [1/4] Checking validated MVP crates...
call "%~dp0build.bat" check -p hive_cloud -p hive_admin -p hive_terminal -p hive_blockchain -p hive_ui_panels -p hive_ui -p hive_app
if errorlevel 1 exit /b %errorlevel%

echo [2/4] Running security-critical crate tests...
call "%~dp0build.bat" test -p hive_core -p hive_agents -q
if errorlevel 1 exit /b %errorlevel%

echo [3/4] Running backend/service tests...
call "%~dp0build.bat" test -p hive_a2a -p hive_cloud -p hive_admin -p hive_cli -p hive_terminal -p hive_blockchain -q
if errorlevel 1 exit /b %errorlevel%

echo [4/4] Running token launch UI tests...
call "%~dp0build.bat" test -p hive_ui --test test_token_launch -q
if errorlevel 1 exit /b %errorlevel%

echo Verification complete.
