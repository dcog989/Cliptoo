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

    try {
        $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
        $logEntry = "[$timestamp] [$Level] $Message"

        # Add buffer overflow protection
        if ($Script:LogBuffer.Count -ge 1000) {
            Write-Log "Log buffer overflow detected, forcing flush" "WARN"
            Sync-LogBuffer
        }

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
    catch {
        # Fallback to console if logging fails
        Write-Host "LOGGING ERROR: $($_.Exception.Message)" -ForegroundColor Red
        Write-Host "Original message: [$Level] $Message" -ForegroundColor Gray
    }
}

function Sync-LogBuffer {
    if ($Script:LogBuffer.Count -gt 0) {
        try {
            $logFile = Get-LogFile

            # Ensure log file exists and is accessible
            if (-not (Test-Path $logFile)) {
                $logDir = Split-Path $logFile -Parent
                if (-not (Test-Path $logDir)) {
                    New-Item -ItemType Directory -Path $logDir -Force -ErrorAction Stop | Out-Null
                }
            }

            # Use file locking for concurrent access protection
            $retryCount = 0
            $maxRetries = 3
            $retryDelay = 100

            while ($retryCount -lt $maxRetries) {
                try {
                    # Try to get exclusive access to the file
                    $fileStream = [System.IO.File]::Open($logFile, [System.IO.FileMode]::Append, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
                    $writer = [System.IO.StreamWriter]::new($fileStream)

                    # Write all lines in one go if buffer is small,
                    # or iterate efficiently to leverage StreamWriter's internal buffering
                    foreach ($entry in $Script:LogBuffer) {
                        $writer.WriteLine($entry)
                    }

                    $writer.Flush()
                    $writer.Close()
                    $fileStream.Close()
                    $Script:LogBuffer.Clear()
                    break
                }
                catch [System.IO.IOException] {
                    $retryCount++
                    if ($retryCount -ge $maxRetries) {
                        # Fallback to non-exclusive write if locking fails
                        Add-Content -Path $logFile -Value $Script:LogBuffer -ErrorAction SilentlyContinue
                        $Script:LogBuffer.Clear()
                        Write-Log "Warning: Could not get exclusive lock on log file, used fallback method" "WARN"
                        break
                    }
                    Start-Sleep -Milliseconds $retryDelay
                    $retryDelay *= 2
                }
                finally {
                    if ($writer) { $writer.Dispose() }
                    if ($fileStream) { $fileStream.Dispose() }
                }
            }
        }
        catch {
            Write-Host "Failed to sync log buffer: $($_.Exception.Message)" -ForegroundColor Red
            # Clear buffer even if write fails to prevent memory buildup
            $Script:LogBuffer.Clear()
        }
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
