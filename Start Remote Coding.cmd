@echo off
if not exist "%~dp0target\release\remote-coding.exe" (
  echo Build the application first. See README_LOCAL.md.
  pause
  exit /b 1
)
start "" "%~dp0target\release\remote-coding.exe"
