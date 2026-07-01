# Builder Toolbox - Utility Functions

function Invoke-Countdown {
    param(
        [int]$Seconds = 3,
        [string]$Message = "Returning to menu"
    )

    Write-Host "$Message" -NoNewline -ForegroundColor DarkGray
    for ($i = 0; $i -lt $Seconds; $i++) {
        Write-Host "." -NoNewline -ForegroundColor DarkGray
        Start-Sleep -Seconds 1
    }
    Write-Host ""
}

function Test-Prerequisites {
    $dotNetCheck = Test-DotNetVersion
    if (-not $dotNetCheck.Success) {
        throw $dotNetCheck.Message
    }

    if (-not (Test-ConfigurationValidity)) {
        throw "Configuration validation failed. Please check Configuration.ps1"
    }

    if ($Script:UseVelopack) {
        if (-not (Get-Command vpk -ErrorAction SilentlyContinue)) {
            Write-Log "Velopack tool 'vpk' not found. Attempting to install it globally..." "WARN"
            $installResult = Invoke-DotnetCommand -Command "tool" -Arguments "install -g vpk"
            if (-not $installResult.Success) {
                throw "Failed to install Velopack tool 'vpk'. Please install it manually with: dotnet tool install -g vpk"
            }
            Write-Log "Velopack tool 'vpk' installed successfully." "SUCCESS"
        }
        else {
            # vpk --version 2>&1 produces ErrorRecords if output goes to stderr (common in PS with native tools)
            # Pipe to Out-String to force conversion to string before trimming.
            $vpkVersionOutput = (vpk --version 2>&1 | Out-String).Trim()
            Write-Log "Velopack version: $vpkVersionOutput" "DEBUG"
        }
    }
}

function Test-ProjectFilesExist {
    if (-not (Test-Path $Script:SolutionFile)) {
        Write-Log "Solution file not found at '$($Script:SolutionFile)'. This operation cannot continue." "ERROR"
        return $false
    }
    if (-not (Test-Path $Script:MainProjectFile)) {
        Write-Log "Main project file not found at '$($Script:MainProjectFile)'. This operation cannot continue." "ERROR"
        return $false
    }
    return $true
}

function Test-PathExists {
    param([string]$Path, [string]$Description)

    if (-not (Test-Path $Path)) {
        Write-Log "$Description not found: $Path" "ERROR"
        return $false
    }
    return $true
}

function Remove-FilesByPattern {
    param([string]$Path, [string[]]$Patterns)

    $files = Get-ChildItem -Path $Path -Include $Patterns -Recurse -File -ErrorAction SilentlyContinue
    if ($files) {
        $files | Remove-Item -Force -ErrorAction SilentlyContinue
        return $files.Count
    }
    return 0
}

function Invoke-WithStandardErrorHandling {
    param(
        [scriptblock]$Action,
        [string]$SuccessMessage,
        [string]$FailureMessage,
        [bool]$LogError = $true
    )

    $result = & $Action
    if ($result.Success) {
        if ($SuccessMessage) { Write-Log $SuccessMessage "SUCCESS" }
        return $result.Data
    }
    else {
        if ($LogError) { Write-Log "$FailureMessage : $($result.Message)" "ERROR" }
        return $null
    }
}

function Clear-ScreenRobust {
    # Standard screen clear is now sufficient since we aren't fighting
    # progress bar artifacts or buffer ghosting anymore.
    try {
        [System.Console]::Clear()
    }
    catch {
        Clear-Host
    }

    # Flushing buffers is still good practice to ensure
    # logs are fully written before the menu redraws.
    [System.Console]::Out.Flush()
    [System.Console]::Error.Flush()
}

function Confirm-IdeShutdown {
    [CmdletBinding()]
    param([string]$Action)

    $ideProcesses = $Script:IdeProcessNames

    $runningIdes = [System.Collections.Generic.List[string]]::new()
    foreach ($procName in $ideProcesses.Keys) {
        if (Get-Process -Name $procName -ErrorAction SilentlyContinue) {
            $runningIdes.Add($ideProcesses[$procName])
        }
    }

    if ($runningIdes.Count -eq 0) {
        return $true
    }

    $ideList = $runningIdes -join ', '
    $prompt = "The following IDE(s) are running: $ideList. Continuing may cause file lock errors. Continue anyway? (y/n)"
    Write-Log "IDE(s) running: $ideList. This may cause the '$Action' operation to fail." "WARN"

    $choice = Read-Host -Prompt $prompt

    if ($choice.ToLower() -eq 'y') {
        Write-Log "User chose to proceed despite running IDEs."
        return $true
    }

    Write-Log "$Action aborted by user due to running IDEs." "ERROR"
    return $false
}

function Stop-ProcessForcefully {
    param(
        [string]$ProcessName,
        [int]$TimeoutSeconds = 10
    )

    try {
        $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
        if ($null -eq $processes -or $processes.Count -eq 0) {
            Write-Log "No processes found with name '$ProcessName'" "DEBUG"
            return $true
        }

        Write-Log "Forcefully terminating $ProcessName (PIDs: $($processes.Id -join ', '))..."

        # Terminate all processes with the specified name
        $processes | ForEach-Object {
            try {
                $_.Kill() | Out-Null
                Write-Log "Sent kill signal to PID $($_.Id)" "DEBUG"
            }
            catch {
                Write-Log "Failed to kill PID $($_.Id): $($_.Exception.Message)" "WARN"
            }
        }

        # Wait for processes to actually terminate
        return Wait-ProcessTermination -ProcessName $ProcessName -TimeoutSeconds $TimeoutSeconds
    }
    catch {
        Write-Log "Error in Stop-ProcessForcefully: $($_.Exception.Message)" "ERROR"
        return $false
    }
}

function Stop-ProcessGracefully {
    param(
        [string]$ProcessName,
        [int]$GracefulTimeoutSeconds = 3,
        [int]$ForcefulTimeoutSeconds = 7
    )

    try {
        $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
        if ($null -eq $processes -or $processes.Count -eq 0) {
            Write-Log "No processes found with name '$ProcessName'" "DEBUG"
            return $true
        }

        Write-Log "Attempting graceful termination of $ProcessName (PIDs: $($processes.Id -join ', '))..."

        # Try graceful close first for GUI processes
        $gracefulAttempts = 0
        foreach ($process in $processes) {
            try {
                # Check if process has a main window (GUI process)
                if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
                    $result = $process.CloseMainWindow()
                    if ($result) {
                        $gracefulAttempts++
                        Write-Log "Sent close signal to PID $($process.Id)" "DEBUG"
                    }
                    else {
                        Write-Log "Process PID $($process.Id) has no main window to close" "DEBUG"
                    }
                }
                else {
                    Write-Log "Process PID $($process.Id) is not a GUI process, will use forceful termination" "DEBUG"
                }
            }
            catch {
                Write-Log "Could not send close signal to PID $($process.Id): $($_.Exception.Message)" "DEBUG"
            }
        }

        # Wait for graceful termination
        if ($gracefulAttempts -gt 0) {
            $gracefulStart = Get-Date
            while (((Get-Date) - $gracefulStart).TotalSeconds -lt $GracefulTimeoutSeconds) {
                $remaining = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
                if ($null -eq $remaining -or $remaining.Count -eq 0) {
                    Write-Log "$ProcessName terminated gracefully" "SUCCESS"
                    return $true
                }
                Start-Sleep -Milliseconds 250
            }
        }

        # Fall back to forceful termination if graceful failed or no GUI processes
        $remainingProcesses = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
        if ($null -ne $remainingProcesses -and $remainingProcesses.Count -gt 0) {
            Write-Log "Graceful termination incomplete, using force for remaining processes..." "WARN"
            return Stop-ProcessForcefully -ProcessName $ProcessName -TimeoutSeconds $ForcefulTimeoutSeconds
        }

        return $true
    }
    catch {
        Write-Log "Error in Stop-ProcessGracefully: $($_.Exception.Message)" "ERROR"
        return $false
    }
}

function Confirm-ProcessTermination {
    param(
        [string]$Action = "Build",
        [int]$TerminationTimeoutSeconds = 10,
        [bool]$UseGracefulTermination = $true
    )

    try {
        $processes = Get-Process -Name $Script:ProcessNameForTermination -ErrorAction SilentlyContinue
        if ($null -eq $processes -or $processes.Count -eq 0) {
            Write-Log "No '$($Script:ProcessNameForTermination)' processes found" "DEBUG"
            return $true
        }

        $pids = $processes.Id -join ', '
        Write-Log "$($Script:ProcessNameForTermination) is running (PID(s): $pids)." "WARN"

        # Add process details for better user understanding
        $processInfo = $processes | ForEach-Object {
            "PID: $($_.Id), Started: $($_.StartTime), CPU: $($_.CPU)ms"
        }
        Write-Log "Process details: $($processInfo -join '; ')" "DEBUG"

        $kill = Read-Host "Do you want to terminate it? (y/n)"
        if ($kill.ToLower() -eq 'y') {
            Write-Log "Terminating $($Script:ProcessNameForTermination) (PIDs: $pids)..."

            # Use the explicit process name for the Stop functions
            $terminated = if ($UseGracefulTermination) {
                Stop-ProcessGracefully -ProcessName $Script:ProcessNameForTermination
            }
            else {
                Stop-ProcessForcefully -ProcessName $Script:ProcessNameForTermination -TimeoutSeconds $TerminationTimeoutSeconds
            }

            if ($terminated) {
                Write-Log "$($Script:ProcessNameForTermination) terminated successfully." "SUCCESS"
                return $true
            }
            else {
                Write-Log "Failed to terminate $($Script:ProcessNameForTermination) after multiple attempts." "ERROR"

                # Provide additional help for stubborn processes
                Write-Host "Tip: You may need to manually close the application or restart your system." -ForegroundColor Yellow
                return $false
            }
        }
        else {
            Write-Log "$Action aborted by user." "ERROR"
            return $false
        }
    }
    catch {
        Write-Log "Error in Confirm-ProcessTermination: $($_.Exception.Message)" "ERROR"
        return $false
    }
}

function Wait-ProcessTermination {
    param(
        [string]$ProcessName,
        [int]$TimeoutSeconds = 10,
        [int]$CheckIntervalMs = 500
    )

    try {
        Write-Log "Waiting for $ProcessName to terminate (timeout: ${TimeoutSeconds}s)..."
        $startTime = Get-Date
        $attempt = 0
        $lastKnownPids = @()

        while ($true) {
            $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue

            if ($null -eq $processes -or $processes.Count -eq 0) {
                $elapsed = [math]::Round(((Get-Date) - $startTime).TotalSeconds, 1)
                Write-Log "$ProcessName terminated after $attempt attempts (${elapsed}s)" "SUCCESS"
                return $true
            }

            $currentPids = $processes.Id
            $elapsed = (Get-Date) - $startTime

            if ($elapsed.TotalSeconds -ge $TimeoutSeconds) {
                $pids = $currentPids -join ', '
                Write-Log "Timeout waiting for $ProcessName to terminate after $TimeoutSeconds seconds" "WARN"
                Write-Log "Processes still running (PIDs: $pids)" "WARN"

                # Provide additional diagnostic information
                $processes | ForEach-Object {
                    Write-Log "Stuck process: PID $($_.Id), Start Time: $($_.StartTime), CPU Time: $($_.CPU)ms" "DEBUG"
                }

                return $false
            }

            # Log progress if PIDs have changed
            if (-not (Compare-Object $currentPids $lastKnownPids -SyncWindow 0)) {
                Write-Log "Still waiting for $ProcessName (PIDs: $($currentPids -join ', '))" "DEBUG"
                $lastKnownPids = $currentPids
            }

            $attempt++
            Start-Sleep -Milliseconds $CheckIntervalMs
        }
    }
    catch {
        Write-Log "Error in Wait-ProcessTermination: $($_.Exception.Message)" "ERROR"
        return $false
    }
}

function Invoke-ItemSafely {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [string]$ItemType = "Item"
    )

    if (-not (Test-Path $Path)) {
        Write-Log "Could not find $ItemType at '$Path'." "ERROR"
        return $false
    }
    else {
        try {
            Invoke-Item $Path
            return $true
        }
        catch {
            Write-Log "Failed to open $ItemType at '$Path': $_" "ERROR"
            return $false
        }
    }
}
