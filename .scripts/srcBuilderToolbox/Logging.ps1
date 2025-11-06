# Builder Toolbox - Logging

function Get-LogFile {
    if ($null -eq $Script:LogFile) {
        $Script:LogFile = Start-Logging
    }
    return $Script:LogFile
}

function Start-Logging {
    $scriptsDir = (Get-Item $PSScriptRoot).Parent.FullName
    $logDir = Join-Path $scriptsDir "Logs"

    if (-not (Test-Path $logDir)) {
        New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    }

    $timestamp = (Get-Date).ToString('yyyyMMdd.HHmmss')
    $logFileName = [string]::Format($Script:LogFileFormat, $Script:PackageTitle, $timestamp)
    $logFile = Join-Path $logDir $logFileName

    $header = @"
***************************************
$($Script:PackageTitle)
Log Start: $((Get-Date).ToString('yyyyMMddHHmmss'))
Username:   $($env:USERDOMAIN)\$($env:USERNAME)
***************************************
"@

    Set-Content -Path $logFile -Value $header
    return $logFile
}

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")

    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logEntry = "[$timestamp] [$Level] $Message"

    $Script:LogBuffer.Add($logEntry)

    if ($Script:LogBuffer.Count -ge $Script:LogBufferFlushThreshold) {
        Sync-LogBuffer
    }

    # Only write specific levels to the console. INFO is now log-only.
    switch ($Level) {
        "ERROR" { Write-Host $Message -ForegroundColor Red }
        "WARN" { Write-Host $Message -ForegroundColor Yellow }
        "SUCCESS" { Write-Host $Message -ForegroundColor Green }
        "CONSOLE" { Write-Host $Message }
    }
}

function Sync-LogBuffer {
    if ($Script:LogBuffer.Count -gt 0) {
        $logFile = Get-LogFile
        Add-Content -Path $logFile -Value $Script:LogBuffer
        $Script:LogBuffer.Clear()
    }
}

function Clear-Logs {
    Write-Log "Cleaning logs..." "CONSOLE"
    $scriptsDir = (Get-Item $PSScriptRoot).Parent.FullName
    $logDir = Join-Path $scriptsDir "Logs"

    if (Test-Path $logDir) {
        $logFiles = Get-ChildItem -Path $logDir -Filter "*.log" -File
        $currentLog = Get-LogFile

        $oldLogs = $logFiles | Where-Object { $_.FullName -ne $currentLog }

        if ($oldLogs.Count -gt 0) {
            Write-Log "Removing $($oldLogs.Count) old log files from $logDir"
            $oldLogs | Remove-Item -Force -ErrorAction SilentlyContinue
            Write-Log "Log cleanup successful." "SUCCESS"
        }
        else {
            Write-Log "No old log files to clean."
        }
    }
    else {
        Write-Log "Log directory not found." "WARN"
    }
}