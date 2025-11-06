# Builder Toolbox - Explorer and IDE Actions

function Open-LatestLogFile {
    $logDir = Join-Path $Script:RepoRoot "Logs"

    if (Test-Path $logDir) {
        $latestLog = Get-ChildItem -Path $logDir -Filter "*.log" -File |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1

        if ($latestLog) {
            $success = Invoke-ItemSafely -Path $latestLog.FullName -ItemType "Log file"
            if (-not $success) {
                return
            }
        }
        else {
            Write-Log "No logs found to open." "WARN"
            Write-Host "No logs found to open." -ForegroundColor Yellow
        }
    }
    else {
        Write-Log "Log directory not found." "WARN"
        Write-Host "Log directory not found." -ForegroundColor Yellow
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