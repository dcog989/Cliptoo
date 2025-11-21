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

    # 1. bin\Debug (Standard/AnyCPU)
    # 2. bin\x64\Debug (Platform specific)
    $searchPaths = @(
        (Join-Path $Script:MainProjectDir "bin\$Configuration"),
        (Join-Path $Script:MainProjectDir "bin\$($Script:BuildPlatform)\$Configuration")
    )

    $exePath = $null

    foreach ($path in $searchPaths) {
        if (Test-Path $path) {
            Write-Log "Searching in: $path" "DEBUG"
            $found = Get-ChildItem -Path $path -Filter "$($Script:AppName).exe" -Recurse -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
            
            if ($found) {
                $exePath = $found
                break
            }
        }
    }

    if ($exePath) {
        Start-Process -FilePath $exePath.FullName
        Write-Log "Application started: $($exePath.FullName)" "SUCCESS"
    }
    else {
        Write-Log "Main application EXE ($($Script:AppName).exe) not found." "ERROR"
        Write-Log "Searched the following locations:" "ERROR"
        $searchPaths | ForEach-Object { Write-Log " - $_" "ERROR" }
    }
}

function Watch-And-Run {
    if (-not (Test-ProjectFilesExist)) { return }
    if (-not (Confirm-ProcessTermination)) { return }

    Write-Log "Starting dotnet watch. Press CTRL+C in the new window to stop." "CONSOLE"
    $arguments = @("watch", "--project", "`"$($Script:MainProjectFile)`"", "run")

    Start-Process -FilePath "dotnet" -ArgumentList $arguments
}