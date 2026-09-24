@echo off
setlocal
pushd "%~dp0"
call cargo build -p newviso -p newviso-bugtrap-ui -j1
if errorlevel 1 (
    popd
    pause
    exit /b 1
)
"%~dp0target\debug\newviso.exe" --base-dir "%~dp0." --project "%~dp0projects\FirstFPS" %*
set "fps_result=%errorlevel%"
popd
exit /b %fps_result%
