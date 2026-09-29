@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-Physics-Game-Shortcuts.ps1"
if errorlevel 1 pause
