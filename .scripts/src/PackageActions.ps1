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
        $arguments = "list `"$($Script:SolutionFile)`" package --outdated"

        Write-Log "Executing: dotnet $arguments"

        # Use synchronous process execution to avoid race conditions with log file writing.
        $processInfo = New-Object System.Diagnostics.ProcessStartInfo
        $processInfo.FileName = "dotnet"
        $processInfo.Arguments = "$arguments --verbosity normal"
        $processInfo.RedirectStandardOutput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true

        $process = [System.Diagnostics.Process]::Start($processInfo)
        $output = $process.StandardOutput.ReadToEnd()
        $stdError = $process.StandardError.ReadToEnd()
        $process.WaitForExit()

        if (-not [string]::IsNullOrWhiteSpace($output)) {
            Add-Content -Path $logFile -Value $output
        }
        if (-not [string]::IsNullOrWhiteSpace($stdError)) {
            Add-Content -Path $logFile -Value "ERROR: $($stdError)"
        }

        # Display a filtered summary to the user for immediate feedback
        if (-not [string]::IsNullOrWhiteSpace($output)) {
            $outputLines = $output.Split([Environment]::NewLine)
            $startIndex = -1
            for ($i = 0; $i -lt $outputLines.Length; $i++) {
                if ($outputLines[$i] -match 'has (no updates|the following updates)') {
                    $startIndex = $i
                    break
                }
            }

            if ($startIndex -ge 0) {
                $summary = $outputLines[$startIndex..($outputLines.Length - 1)] -join [Environment]::NewLine
                Write-Host $summary.Trim()
            }
            else {
                Write-Log "Could not find package update summary in the output. See log for details." "WARN"
            }
        }
        if (-not [string]::IsNullOrWhiteSpace($stdError)) {
            Write-Host $stdError -ForegroundColor Red
        }

        if ($process.ExitCode -eq 0) {
            Write-Log "Package check successful." "SUCCESS"
        }
        else {
            Write-Log "Package check failed. Exit code: $($process.ExitCode)" "ERROR"
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