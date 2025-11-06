# Builder Toolbox - Root Launcher
# This script executes the main script located in the 'src' subfolder,
# allowing the root directory to remain clean.

# Construct the path to the main script inside the 'src' directory.
$mainScriptPath = Join-Path $PSScriptRoot "srcBuilderToolbox\BuilderToolbox.ps1"

if (-not (Test-Path $mainScriptPath)) {
    Write-Host "FATAL ERROR: The main script could not be found at '$mainScriptPath'." -ForegroundColor Red
    Write-Host "Please ensure the 'src' folder and its contents are intact." -ForegroundColor Red
    Read-Host "Press ENTER to exit"
    exit 1
}

# Execute the main script, passing along any arguments.
& $mainScriptPath @args