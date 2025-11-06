# Builder Toolbox - Build Actions

function Start-BuildAndRun {
    param([string]$Configuration)

    if (-not (Test-ProjectFilesExist)) { return }
    if (-not (Confirm-ProcessTermination)) { return }

    Write-Log "Building solution in $Configuration mode for $($Script:BuildPlatform) platform..." "CONSOLE"

    $arguments = "`"$Script:SolutionFile`" -c $Configuration -p:Platform=$($Script:BuildPlatform)"
    $buildResult = Invoke-DotnetCommand -Command "build" -Arguments $arguments
    if (-not $buildResult.Success) {
        Write-Log "Build failed: $($buildResult.Message)" "ERROR"
        return
    }

    Write-Log "Build successful. Searching for application executable..." "SUCCESS"

    # Refactor: Search for the EXE to avoid brittle path construction
    $exeSearchPath = Join-Path $Script:MainProjectDir "bin"
    $exePath = Get-ChildItem -Path $exeSearchPath -Filter "$($Script:AppName).exe" -Recurse -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

    if ($exePath) {
        Start-Process -FilePath $exePath.FullName
        Write-Log "Application started: $($exePath.FullName)" "SUCCESS"
    }
    else {
        Write-Log "Main application EXE not found in $exeSearchPath" "ERROR"
    }
}

function Watch-And-Run {
    if (-not (Test-ProjectFilesExist)) { return }
    if (-not (Confirm-ProcessTermination)) { return }

    Write-Log "Starting dotnet watch. Press CTRL+C in the new window to stop." "CONSOLE"
    $arguments = @("watch", "--project", "`"$($Script:MainProjectFile)`"", "run")

    Start-Process -FilePath "dotnet" -ArgumentList $arguments
}