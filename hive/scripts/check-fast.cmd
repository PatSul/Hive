@echo off
setlocal

pushd "%~dp0.." >nul
if errorlevel 1 exit /b %errorlevel%

echo == cargo check: hive_ui ==
cargo check -p hive_ui --message-format=short -j 1
if errorlevel 1 goto fail

echo.
echo == cargo check: hive_app ==
cargo check -p hive_app --message-format=short -j 1
if errorlevel 1 goto fail

echo.
echo == cargo check: hive_ai ==
cargo check -p hive_ai --message-format=short -j 1
if errorlevel 1 goto fail

echo.
echo == git diff --check ==
git diff --check
if errorlevel 1 goto fail

popd >nul
echo.
echo Fast checks passed.
exit /b 0

:fail
set STATUS=%errorlevel%
popd >nul
exit /b %STATUS%
