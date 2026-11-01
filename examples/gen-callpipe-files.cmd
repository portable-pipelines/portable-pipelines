@echo off
rem Cmd-friendly wrapper for gen-callpipe-files.ps1 (PowerShell 5.1).
rem   examples\gen-callpipe-files.cmd -Files 50 -Lines 4 -Width 48
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0gen-callpipe-files.ps1" %*
exit /b %ERRORLEVEL%
