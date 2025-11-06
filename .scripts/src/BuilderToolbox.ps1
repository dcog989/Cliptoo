# Builder Toolbox for .NET Apps - Startup Script
# Loads the core module and executes the main function.
# version: 1.16.0
# -----------------------------------------------------------------------------

try {
    # Define the path to the module file.
    $modulePath = Join-Path $PSScriptRoot "BuilderToolbox.psm1"

    if (-not (Test-Path $modulePath)) {
        throw "Error: BuilderToolbox.psm1 module not found at '$modulePath'."
    }

    # Import the module (imports its functions into the current session)
    Import-Module $modulePath -Force -ErrorAction Stop

    # Execute the main function exported by the module
    Main
}
catch {
    Write-Host "--------------------------------------------------------" -ForegroundColor Red
    Write-Host "A critical error occurred during startup:" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    Write-Host "--------------------------------------------------------" -ForegroundColor Red
    Read-Host "Press ENTER to exit"
    exit 1
}