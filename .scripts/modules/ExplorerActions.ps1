# Builder Toolbox - Explorer and IDE Actions

function Open-LatestLogFile {
    # 1. Ensure any pending log entries are flushed to disk before opening
    if (Get-Command Sync-LogBuffer -ErrorAction SilentlyContinue) {
        Sync-LogBuffer
    }

    # 2. Attempt to use the currently active log file path first
    $logPath = $Script:LogFile

    # 3. Fallback: If no active log session exists, search the directory
    if (-not $logPath -or -not (Test-Path $logPath)) {
        # Standardize the log directory calculation to match Logging.ps1
        $scriptsDir = (Get-Item $PSScriptRoot).Parent.FullName
        $logDir = Join-Path $scriptsDir "Logs"

        if (Test-Path $logDir) {
            $latestLog = Get-ChildItem -Path $logDir -Filter "*.log" -File |
                Sort-Object LastWriteTime -Descending |
                Select-Object -First 1

            if ($latestLog) {
                $logPath = $latestLog.FullName
            }
        }
    }

    # 4. Open the file if found
    if ($logPath -and (Test-Path $logPath)) {
        $success = Invoke-ItemSafely -Path $logPath -ItemType "Log file"
        if (-not $success) {
            return
        }
    }
    else {
        Write-Log "No log file found to open." "WARN"
    }
}

function Open-UserDataFolder {
    if (-not (Test-ProjectFilesExist)) { return }
    $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent

    # The portable version is staged in the 'publish' subdirectory during a portable build.
    $publishPath = Join-Path $mainProjectDir "bin\$($Script:BuildPlatform)\Release\$($Script:TargetFramework)\$($Script:PublishRuntimeId)\publish"
    $portableMarkerPath = Join-Path $publishPath $Script:PortableMarkerFile

    $userDataPath = if (Test-Path $portableMarkerPath) {
        Join-Path $publishPath "Data"
    }
    else {
        Join-Path $env:APPDATA $Script:AppDataFolderName
    }

    $success = Invoke-ItemSafely -Path $userDataPath -ItemType "User data folder"
    if (-not $success) {
        return
    }
}

function Open-OutputFolder {
    if (-not (Test-ProjectFilesExist)) { return }
    $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent
    $outputPath = Join-Path $mainProjectDir "bin"
    $success = Invoke-ItemSafely -Path $outputPath -ItemType "Output folder"
    if (-not $success) {
        return
    }
}

function Open-SolutionInIDE {
    if (-not (Test-ProjectFilesExist)) { return }
    $success = Invoke-ItemSafely -Path $Script:SolutionFile -ItemType "Solution file"
    if (-not $success) {
        return
    }
}
