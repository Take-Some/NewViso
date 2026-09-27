@echo off
setlocal
call "%~dp0..\Scripts\Run\RunFirstFPS-Debug.cmd"
exit /b %ERRORLEVEL%
