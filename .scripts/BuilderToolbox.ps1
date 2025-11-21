# Builder Toolbox - Root Launcher
# A build script for .NET apps. Cleans, builds, logs.
# version: 1.17.0
# This script executes the main script located in the 'modules' subfolder.

try {
    # Construct the path to the main script inside the 'modules' directory.
    $mainScriptPath = Join-Path $PSScriptRoot "modules\BuilderToolboxStartup.ps1"

    # Resolve the absolute path to handle relative path issues
    $resolvedPath = Resolve-Path -Path $mainScriptPath -ErrorAction Stop

    if (-not (Test-Path $resolvedPath)) {
        throw "The main script could not be found at '$resolvedPath'."
    }

    # Execute the main script, passing along any arguments.
    & $resolvedPath @args
}
catch {
    Write-Host "--------------------------------------------------------" -ForegroundColor Red
    Write-Host "FATAL ERROR: Failed to launch Builder Toolbox" -ForegroundColor Red
    Write-Host "Error: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "Expected path: $mainScriptPath" -ForegroundColor Red
    Write-Host "Please ensure the 'modules' folder and its contents are intact." -ForegroundColor Red
    Write-Host "--------------------------------------------------------" -ForegroundColor Red
    Read-Host "Press ENTER to exit"
    exit 1
}
