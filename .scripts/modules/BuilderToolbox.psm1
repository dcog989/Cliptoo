# Builder Toolbox for .NET Apps - Module Definition
# This file contains the main execution logic and menu system.
# Dependencies are now loaded via 'NestedModules' in BuilderToolbox.psd1.

# --- Menu Display ---

$Script:MenuLayoutCache = $null

function Show-Menu {
    if (-not (Test-Path $Script:SolutionFile)) {
        Write-Host "-------------------------------------------------------------------------------" -ForegroundColor Yellow
        Write-Host " WARNING: Solution file not found. Project-related features are disabled." -ForegroundColor Yellow
    }

    $appVersion = Get-CachedBuildVersion  # Use cached version
    if ([string]::IsNullOrEmpty($appVersion)) {
        $appVersion = "N/A (check $($Script:MainProjectFile))"
    }

    $logFileName = if ($Script:LogFile) {
        Split-Path $Script:LogFile -Leaf
    }
    else {
        "Not created yet."
    }

    $gitInfo = Get-GitInfo # Now uses cached version

    Write-Host "-------------------------------------------------------------------------------" -ForegroundColor Green
    Write-Host "              .NET Builder Toolbox for $($Script:PackageTitle)" -ForegroundColor Green
    Write-Host "-------------------------------------------------------------------------------" -ForegroundColor Green
    Write-Host "Solution: $($Script:SolutionFile)" -ForegroundColor DarkGray

    if ($gitInfo.Branch -ne "N/A" -and $gitInfo.Commit -ne "N/A") {
        Write-Host "Branch:   $($gitInfo.Branch) ($($gitInfo.Commit))" -ForegroundColor DarkGray
    }

    Write-Host "Version:  $appVersion" -ForegroundColor DarkGray
    Write-Host "SDK:      $($Script:SdkVersion)" -ForegroundColor DarkGray
    Write-Host "Logging:  $logFileName" -ForegroundColor DarkGray
    Write-Host "-------------------------------------------------------------------------------" -ForegroundColor Green

    if ($null -eq $Script:MenuLayoutCache) {
        $numericKeys = $Script:MenuItems.Keys | Where-Object { $_ -match '^\d+$' } | Sort-Object
        $alphaKeys = $Script:MenuItems.Keys | Where-Object { $_ -match '^[A-Z]$' } | Sort-Object
        $maxRows = [math]::Max($numericKeys.Count, $alphaKeys.Count)

        # Calculate padding for the left column
        $leftColumnItems = @()
        foreach ($key in $numericKeys) {
            $leftColumnItems += "$key. $($Script:MenuItems[$key].Description)"
        }
        $maxLeftWidth = 0
        if ($leftColumnItems.Count -gt 0) {
            $maxLeftWidth = ($leftColumnItems | Measure-Object -Property Length -Maximum).Maximum
        }
        $columnPadding = $maxLeftWidth + 4 # 4 spaces for gutter

        $Script:MenuLayoutCache = [PSCustomObject]@{
            NumericKeys   = $numericKeys
            AlphaKeys     = $alphaKeys
            MaxRows       = $maxRows
            ColumnPadding = $columnPadding
        }
    }

    $layout = $Script:MenuLayoutCache

    for ($i = 0; $i -lt $layout.MaxRows; $i++) {
        $left = ""
        if ($i -lt $layout.NumericKeys.Count) {
            $key = $layout.NumericKeys[$i]
            $left = "$key. $($Script:MenuItems[$key].Description)"
        }

        $right = ""
        if ($i -lt $layout.AlphaKeys.Count) {
            $key = $layout.AlphaKeys[$i]
            $right = "$key. $($Script:MenuItems[$key].Description)"
        }

        Write-Host ($left.PadRight($layout.ColumnPadding) + $right)
    }

    Write-Host "Q. Quit" -ForegroundColor Magenta
    Write-Host "-------------------------------------------------------------------------------" -ForegroundColor Green
}

function Invoke-MenuChoice {
    param(
        [string]$Choice,
        [ref]$ExitRef
    )

    $choiceKey = $Choice.ToUpper()

    if ($choiceKey -eq 'Q') {
        $ExitRef.Value = $true
        return "NoPause"
    }

    if ($Script:MenuItems.Contains($choiceKey)) {
        $menuItem = $Script:MenuItems[$choiceKey]
        Write-Log "User selected option: '$Choice' ($($menuItem.Description))"

        $commandName = $menuItem.Command
        $argsDict = $menuItem.Args

        if (Get-Command $commandName -ErrorAction SilentlyContinue) {
            if ($argsDict -and $argsDict.Count -gt 0) {
                & $commandName @argsDict
            }
            else {
                & $commandName
            }
        }
        else {
            Write-Log "Menu Action Error: Command '$commandName' not found." "ERROR"
        }

        return $menuItem.Response
    }
    else {
        Write-Log "User selected invalid option: '$Choice'" "WARN"
        return "PauseBriefly"
    }
}

function Invoke-MenuResponse {
    param([string]$ResponseType)

    switch ($ResponseType) {
        "WaitForEnter" {
            Read-Host "ENTER to continue"
        }
        "PauseBriefly" {
            Invoke-Countdown -Seconds 3 -Message "Returning to menu"
        }
        "NoPause" {
            # No action needed
        }
        default {
            Read-Host "ENTER to continue"
        }
    }
}

# --- Main Execution Logic ---
function Main {
    try {
        Write-Log "Starting Builder Toolbox main execution" "INFO"

        # Test prerequisites with detailed error reporting
        try {
            Test-Prerequisites
            Write-Log "Pre-flight checks passed." "SUCCESS"
        }
        catch {
            Write-Log "Pre-flight validation failed: $($_.Exception.Message)" "ERROR"
            throw "Pre-flight validation failed. Please check your configuration and environment setup."
        }

        # Determine the effective application name for use throughout the script.
        try {
            $Script:AppName = Get-EffectiveAppName
            $Script:ProcessNameForTermination = $Script:AppName # Set process name for termination
            Write-Log "Effective App Name set to: $($Script:AppName)" "INFO"

            if ([string]::IsNullOrEmpty($Script:AppName)) {
                throw "Failed to determine effective application name."
            }
        }
        catch {
            Write-Log "Failed to set application name: $($_.Exception.Message)" "ERROR"
            throw "Application configuration error: Unable to determine application name."
        }

        $exit = $false
        $firstRun = $true
        $menuLoopCount = 0
        $maxMenuLoops = 1000  # Prevent infinite loops

        while (-not $exit -and $menuLoopCount -lt $maxMenuLoops) {
            try {
                if (-not $firstRun) {
                    Clear-ScreenRobust
                }
                $firstRun = $false
                $menuLoopCount++

                Show-Menu
                Write-Host "Run option: " -ForegroundColor Cyan -NoNewline
                $choice = Read-Host
                Write-Host "=============" -ForegroundColor Cyan

                $responseType = Invoke-MenuChoice -Choice $choice -ExitRef ([ref]$exit)

                if (-not $exit) {
                    Invoke-MenuResponse -ResponseType $responseType
                }
            }
            catch {
                Write-Log "Error in menu loop iteration $($menuLoopCount): $($_.Exception.Message)" "ERROR"
                Write-Host "An error occurred in the menu. Press ENTER to continue..." -ForegroundColor Red
                Read-Host
                # Continue the loop despite the error
            }
        }

        if ($menuLoopCount -ge $maxMenuLoops) {
            Write-Log "Maximum menu loop iterations reached, exiting to prevent infinite loop" "WARN"
            Write-Host "Maximum menu iterations reached. Exiting for safety." -ForegroundColor Yellow
        }

        Write-Log "Main execution completed successfully" "SUCCESS"
    }
    catch {
        Write-Log "Script terminating with critical error: $($_.Exception.Message)" "ERROR"

        # Provide detailed error information to user
        Write-Host "--------------------------------------------------------" -ForegroundColor Red
        Write-Host "CRITICAL ERROR: Builder Toolbox cannot continue" -ForegroundColor Red
        Write-Host "Error: $($_.Exception.Message)" -ForegroundColor Red

        if ($_.Exception.InnerException) {
            Write-Host "Details: $($_.Exception.InnerException.Message)" -ForegroundColor DarkGray
        }

        Write-Host "--------------------------------------------------------" -ForegroundColor Red
        Write-Host "Please check the log file for more details:" -ForegroundColor Yellow
        try {
            $logFile = Get-LogFile
            Write-Host "Log file: $logFile" -ForegroundColor Yellow
        }
        catch {
            Write-Host "Log file: Unable to determine log file location" -ForegroundColor Red
        }
        Write-Host "--------------------------------------------------------" -ForegroundColor Red

        Read-Host "`nPress ENTER to exit"
        exit 1
    }
    finally {
        try {
            Write-Log "Exiting script. Cleaning up resources..." "INFO"
            Sync-LogBuffer

            # Additional cleanup if needed
            try {
                $logBuffer = $Script:LogBuffer
                if ($logBuffer -and $logBuffer.Count -gt 0) {
                    Write-Log "Flushing remaining log entries before exit" "DEBUG"
                    Sync-LogBuffer
                }
            }
            catch {
                # Ignore errors during final cleanup
            }
        }
        catch {
            # Ignore cleanup errors during exit
            Write-Host "Warning: Error during cleanup: $($_.Exception.Message)" -ForegroundColor Yellow
        }
    }
}

# Export only the functions needed for the user to run the tool (mainly Main).
# This MUST come after the function definitions.
Export-ModuleMember -Function Main, Show-Menu