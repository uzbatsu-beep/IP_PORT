# Requires PowerShell 5.1+ and Python 3.10+.
$ErrorActionPreference = 'Stop'
& python (Join-Path $PSScriptRoot 'preflight.py') @args
exit $LASTEXITCODE
