# Builder Toolbox - Utility Functions

function Test-Prerequisites {
    $dotNetCheck = Test-DotNetVersion
    if (-not $dotNetCheck.Success) {
        throw $dotNetCheck.Message
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
    # Force clear all progress bars by completing them
    try {
        Write-Progress -Completed
        Write-Progress -Activity " " -Status " " -Completed
        # Add a small delay to ensure progress bars are cleared
        Start-Sleep -Milliseconds 50
    }
    catch { }

    # Clear the screen using the most reliable method
    try {
        # This is the most reliable way to clear the screen in PowerShell
        $Host.UI.RawUI.Clear()
    }
    catch {
        try {
            Clear-Host
        }
        catch {
            try {
                [System.Console]::Clear()
            }
            catch {
                # Nuclear option - write enough blank lines to push everything out
                Write-Host ("`n" * 50)
            }
        }
    }

    # Ensure output is flushed
    [System.Console]::Out.Flush()
    [System.Console]::Error.Flush()
}

function Confirm-IdeShutdown {
    [CmdletBinding()]
    param([string]$Action)

    $ideProcesses = @{
        "devenv"  = "Visual Studio";
        "Code"    = "Visual Studio Code";
        "rider64" = "JetBrains Rider"
    }

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

    $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
    if ($processes.Count -eq 0) { return $true }

    Write-Log "Forcefully terminating $ProcessName..."
    $processes | Stop-Process -Force -ErrorAction SilentlyContinue
    return Wait-ProcessTermination -ProcessName $ProcessName -TimeoutSeconds $TimeoutSeconds
}

function Stop-ProcessGracefully {
    param(
        [string]$ProcessName,
        [int]$GracefulTimeoutSeconds = 3,
        [int]$ForcefulTimeoutSeconds = 7
    )

    $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
    if ($processes.Count -eq 0) {
        return $true
    }

    Write-Log "Attempting graceful termination of $ProcessName..."

    # Try graceful close first
    foreach ($process in $processes) {
        try {
            $process.CloseMainWindow() | Out-Null
        }
        catch {
            Write-Log "Could not send close signal to PID $($process.Id): $_" "DEBUG"
        }
    }

    $gracefulStart = Get-Date
    while (((Get-Date) - $gracefulStart).TotalSeconds -lt $GracefulTimeoutSeconds) {
        $remaining = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
        if ($remaining.Count -eq 0) {
            Write-Log "$ProcessName terminated gracefully" "SUCCESS"
            return $true
        }
        Start-Sleep -Milliseconds 250
    }

    # Fall back to forceful termination
    Write-Log "Graceful termination failed, using force..." "WARN"
    return Stop-ProcessForcefully -ProcessName $ProcessName -TimeoutSeconds $ForcefulTimeoutSeconds
}

function Confirm-ProcessTermination {
    param(
        [string]$Action = "Build",
        [int]$TerminationTimeoutSeconds = 10,
        [bool]$UseGracefulTermination = $true
    )

    # Use a wildcard search to address potential bug with process name matching
    $processFilter = "$($Script:ProcessNameForTermination)*"
    $processes = Get-Process -Name $processFilter -ErrorAction SilentlyContinue
    if ($processes.Count -eq 0) {
        return $true
    }

    $pids = $processes.Id -join ', '
    Write-Log "$Script:ProcessNameForTermination is running (PID(s): $pids)." "WARN"

    $kill = Read-Host "Do you want to terminate it? (y/n)"
    if ($kill.ToLower() -eq 'y') {
        Write-Log "Terminating $Script:ProcessNameForTermination (PIDs: $pids)..."

        # Use the explicit process name for the Stop functions
        $terminated = if ($UseGracefulTermination) {
            Stop-ProcessGracefully -ProcessName $Script:ProcessNameForTermination
        }
        else {
            Stop-ProcessForcefully -ProcessName $Script:ProcessNameForTermination -TimeoutSeconds $TerminationTimeoutSeconds
        }

        if ($terminated) {
            Write-Log "$Script:ProcessNameForTermination terminated." "SUCCESS"
            return $true
        }
        else {
            Write-Log "Failed to terminate $Script:ProcessNameForTermination after multiple attempts." "ERROR"
            return $false
        }
    }
    else {
        Write-Log "$Action aborted." "ERROR"
        return $false
    }
}

function Wait-ProcessTermination {
    param(
        [string]$ProcessName,
        [int]$TimeoutSeconds = 10,
        [int]$CheckIntervalMs = 500
    )

    Write-Log "Waiting for $ProcessName to terminate (timeout: ${TimeoutSeconds}s)..."
    $startTime = Get-Date
    $attempt = 0

    while ($true) {
        $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
        if ($processes.Count -eq 0) {
            Write-Log "$ProcessName terminated after $attempt attempts"
            return $true
        }

        $elapsed = (Get-Date) - $startTime
        if ($elapsed.TotalSeconds -ge $TimeoutSeconds) {
            $pids = $processes.Id -join ', '
            Write-Log "Timeout waiting for $ProcessName to terminate after $TimeoutSeconds seconds" "WARN"
            Write-Log "Processes still running (PIDs: $pids)" "WARN"
            return $false
        }

        $attempt++
        Start-Sleep -Milliseconds $CheckIntervalMs
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
        Write-Host "Could not find $ItemType at '$Path'." -ForegroundColor Red
        return $false
    }
    else {
        try {
            Invoke-Item $Path
            return $true
        }
        catch {
            Write-Log "Failed to open $ItemType at '$Path': $_" "ERROR"
            Write-Host "Failed to open $ItemType at '$Path'." -ForegroundColor Red
            return $false
        }
    }
}