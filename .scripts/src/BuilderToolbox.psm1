# Builder Toolbox for .NET Apps - Module Definition
# This file loads all component scripts and exports the main functions.

# Dot-source all component files to bring their functions and variables into the module scope.
# The order is important: Classes/Config must be first, then Logging, then Utilities.
. (Join-Path $PSScriptRoot "Classes.ps1")
. (Join-Path $PSScriptRoot "Configuration.ps1")
. (Join-Path $PSScriptRoot "Logging.ps1")
. (Join-Path $PSScriptRoot "Utilities.ps1")
. (Join-Path $PSScriptRoot "Dotnet.ps1")
. (Join-Path $PSScriptRoot "GitAndTools.ps1")
. (Join-Path $PSScriptRoot "BuildActions.ps1")
. (Join-Path $PSScriptRoot "PackageActions.ps1")
. (Join-Path $PSScriptRoot "MaintenanceActions.ps1")
. (Join-Path $PSScriptRoot "PublishActions.ps1")
. (Join-Path $PSScriptRoot "ExplorerActions.ps1")

# --- Menu Display ---
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

    $gitInfo = Get-GitInfo # Ensure Git info is fetched before display

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

    $numericKeys = $Script:MenuItems.Keys | Where-Object { $_ -match '^\d+$' } | Sort-Object
    $alphaKeys = $Script:MenuItems.Keys | Where-Object { $_ -match '^[A-Z]$' } | Sort-Object
    $maxRows = [math]::Max($numericKeys.Count, $alphaKeys.Count)

    $menuTable = for ($i = 0; $i -lt $maxRows; $i++) {
        $left = ""
        if ($i -lt $numericKeys.Count) {
            $key = $numericKeys[$i]
            $left = "$key. $($Script:MenuItems[$key].Description)"
        }

        $right = ""
        if ($i -lt $alphaKeys.Count) {
            $key = $alphaKeys[$i]
            $right = "$key. $($Script:MenuItems[$key].Description)"
        }
        [PSCustomObject]@{ Left = $left; Right = $right }
    }

    ($menuTable | Format-Table -HideTableHeaders -AutoSize | Out-String).Trim() | Write-Host

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
        & $menuItem.Action
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
            Write-Host "Returning to menu..." -ForegroundColor DarkGray
            Start-Sleep -Seconds 3
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
        Test-Prerequisites

        # Determine the effective application name for use throughout the script.
        $Script:AppName = Get-EffectiveAppName
        $Script:ProcessNameForTermination = $Script:AppName # Set process name for termination
        Write-Log "Effective App Name set to: $($Script:AppName)"

        $exit = $false
        $firstRun = $true

        while (-not $exit) {
            if (-not $firstRun) {
                Clear-ScreenRobust
            }
            $firstRun = $false

            Show-Menu
            Write-Host "Run option: " -ForegroundColor Cyan -NoNewline
            $choice = Read-Host
            Write-Host "=============" -ForegroundColor Cyan

            $responseType = Invoke-MenuChoice -Choice $choice -ExitRef ([ref]$exit)

            if (-not $exit) {
                Invoke-MenuResponse -ResponseType $responseType
            }
        }
    }
    catch {
        Write-Log "Script terminating with error: $($_.Exception.Message)" "ERROR"
        Read-Host "`nPress ENTER to exit"
    }
    finally {
        Write-Log "Exiting script."
    }
}

# Export only the functions needed for the user to run the tool (mainly Main).
# This MUST come after the function definitions.
Export-ModuleMember -Function Main, Show-Menu