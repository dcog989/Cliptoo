# Builder Toolbox - NuGet Package Actions

function Get-OutdatedPackages {
    if (-not (Test-ProjectFilesExist)) { return }
    Write-Log "Ensuring packages are restored first..." "CONSOLE"

    $restoreResult = Invoke-DotnetCommand -Command "restore" -Arguments "`"$Script:SolutionFile`""
    if (-not $restoreResult.Success) {
        Write-Log "Package restore failed: $($restoreResult.Message)" "ERROR"
        return
    }

    Write-Log "Checking for outdated NuGet packages..." "CONSOLE"

    $process = $null
    try {
        $logFile = Get-LogFile
        $arguments = "list `"$($Script:SolutionFile)`" package --outdated --format json"

        Write-Log "Executing: dotnet $arguments"

        $processInfo = New-Object System.Diagnostics.ProcessStartInfo
        $processInfo.FileName = "dotnet"
        $processInfo.Arguments = "$arguments" # Verbosity not needed for JSON
        $processInfo.RedirectStandardOutput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true

        $process = [System.Diagnostics.Process]::Start($processInfo)
        $output = $process.StandardOutput.ReadToEnd()
        $stdError = $process.StandardError.ReadToEnd()
        $process.WaitForExit()

        if (-not [string]::IsNullOrWhiteSpace($stdError)) {
            Add-Content -Path $logFile -Value "ERROR: $($stdError)"
            Write-Host $stdError -ForegroundColor Red
        }

        if ($process.ExitCode -eq 0 -and -not [string]::IsNullOrWhiteSpace($output)) {
            try {
                # Extract JSON content only (ignoring potential MSBuild output/banners)
                $jsonContent = $output
                if ($output -match '(?ms)(\{.*\})') {
                    $jsonContent = $matches[1]
                }

                $json = $jsonContent | ConvertFrom-Json
                $updatesFound = $false
                $summary = [System.Text.StringBuilder]::new()

                if ($json.projects) {
                    foreach ($project in $json.projects) {
                        foreach ($framework in $project.frameworks) {
                            foreach ($pkg in $framework.topLevelPackages) {
                                $updatesFound = $true
                                $line = "Project: $(Split-Path $project.path -Leaf) | Package: $($pkg.id) | Current: $($pkg.resolvedVersion) -> Latest: $($pkg.latestVersion)"
                                [void]$summary.AppendLine($line)
                            }
                        }
                    }
                }

                if ($updatesFound) {
                    $msg = "Updates available:`n" + $summary.ToString()
                    Write-Host $msg -ForegroundColor Yellow
                    Add-Content -Path $logFile -Value $msg
                }
                else {
                    Write-Log "No outdated packages found." "SUCCESS"
                }
            }
            catch {
                Write-Log "Failed to parse package JSON output: $_" "ERROR"
                Add-Content -Path $logFile -Value "Raw Output: $output"
            }
        }
        else {
            Write-Log "Package check failed or returned no output. Exit code: $($process.ExitCode)" "ERROR"
        }
    }
    catch {
        Write-Log "Failed to execute package check: $_" "ERROR"
    }
    finally {
        if ($process) { $process.Dispose() }
    }
}

function Restore-NuGetPackages {
    if (-not (Test-ProjectFilesExist)) { return }
    if (-not (Confirm-IdeShutdown -Action "Restore NuGet Packages")) { return }

    Write-Log "Restoring NuGet packages..." "CONSOLE"

    $result = Invoke-DotnetCommand -Command "restore" -Arguments "`"$Script:SolutionFile`""

    Clear-BuildVersionCache

    if ($result.Success) {
        Write-Log "NuGet packages restored." "SUCCESS"
    }
    else {
        Write-Log "NuGet packages restore failed: $($result.Message)" "ERROR"
    }
}

function Show-OutdatedPackages {
    Get-OutdatedPackages
    Invoke-ItemSafely -Path (Get-LogFile) -ItemType "Log file"
}