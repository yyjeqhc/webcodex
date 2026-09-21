@echo off
setlocal EnableDelayedExpansion
set "WEBPI_ROOT=%~dp0"
set "WEBPI_SCRIPT=%WEBPI_ROOT%scripts\webpi\standalone.py"

if exist "C:\Python314\python.exe" (
  "C:\Python314\python.exe" -I -B "%WEBPI_SCRIPT%" %*
  exit /b !ERRORLEVEL!
)

where py >nul 2>nul
if %ERRORLEVEL% EQU 0 (
  py -3 -I -B "%WEBPI_SCRIPT%" %*
  exit /b !ERRORLEVEL!
)

where python >nul 2>nul
if %ERRORLEVEL% EQU 0 (
  python -I -B "%WEBPI_SCRIPT%" %*
  exit /b !ERRORLEVEL!
)

echo WebPi requires Python 3 to launch. 1>&2
exit /b 1
