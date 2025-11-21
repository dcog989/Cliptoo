# Builder Toolbox for .NET Apps - Startup Script
# Loads the core module and executes the main function.
# -----------------------------------------------------------------------------

try {
    # Import via Manifest (.psd1) instead of Script Module (.psm1)
    # This ensures metadata is loaded and strict versioning/guid rules can be applied.
    $modulePath = Join-Path $PSScriptRoot "BuilderToolbox.psd1"

    # Validate module path exists
    if (-not (Test-Path $modulePath)) {
        # Fallback for backward compatibility if .psd1 is missing
        $modulePath = Join-Path $PSScriptRoot "BuilderToolbox.psm1"
        if (-not (Test-Path $modulePath)) {
            Write-Host "Error: BuilderToolbox module not found at '$modulePath'." -ForegroundColor Red
            throw "Error: BuilderToolbox module not found at '$modulePath'."
        }
    }

    # Import the module with comprehensive error handling
    try {
        Import-Module $modulePath -Force -ErrorAction Stop
    }
    catch {
        throw "Failed to import BuilderToolbox module: $($_.Exception.Message)"
    }

    # Validate that Main function exists
    if (-not (Get-Command Main -ErrorAction SilentlyContinue)) {
        throw "Main function not found in imported module."
    }

    # Execute the main function exported by the module
    try {
        Main
    }
    catch {
        Write-Host "--------------------------------------------------------" -ForegroundColor Red
        Write-Host "A critical error occurred during execution:" -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor Red

        # Show stack trace for debugging
        if ($_.Exception.StackTrace) {
            Write-Host "Stack trace:" -ForegroundColor DarkGray
            Write-Host $_.Exception.StackTrace -ForegroundColor DarkGray
        }

        Write-Host "--------------------------------------------------------" -ForegroundColor Red

        # Try to log the error if logging is available
        try {
            if (Get-Command Write-Log -ErrorAction SilentlyContinue) {
                Write-Log "Critical error in Main: $($_.Exception.Message)" "ERROR"
                if ($_.Exception.StackTrace) {
                    Write-Log "Stack trace: $($_.Exception.StackTrace)" "DEBUG"
                }
            }
        }
        catch {
            # Logging failed, just continue to exit
        }

        Read-Host "Press ENTER to exit"
        exit 1
    }
}
catch {
    Write-Host "--------------------------------------------------------" -ForegroundColor Red
    Write-Host "A critical error occurred during startup:" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red

    # Show additional context for startup errors
    Write-Host "Context: $($_.Exception.GetType().Name)" -ForegroundColor DarkGray
    if ($_.Exception.InnerException) {
        Write-Host "Inner exception: $($_.Exception.InnerException.Message)" -ForegroundColor DarkGray
    }

    Write-Host "--------------------------------------------------------" -ForegroundColor Red
    Write-Host "Troubleshooting tips:" -ForegroundColor Yellow
    Write-Host "1. Ensure all PowerShell files are present in the modules folder" -ForegroundColor Yellow
    Write-Host "2. Check that PowerShell execution policy allows script execution" -ForegroundColor Yellow
    Write-Host "3. Verify .NET SDK is installed and accessible" -ForegroundColor Yellow
    Write-Host "4. Check that Configuration.ps1 has valid settings" -ForegroundColor Yellow
    Write-Host "--------------------------------------------------------" -ForegroundColor Red

    Read-Host "Press ENTER to exit"
    exit 1
}
finally {
    # Ensure cleanup happens even if startup fails
    try {
        if ((Get-Command Sync-LogBuffer -ErrorAction SilentlyContinue) -and
            (Get-Variable -Name "Script:LogBuffer" -ErrorAction SilentlyContinue)) {
            Sync-LogBuffer
        }
    }
    catch {
        # Ignore cleanup errors during startup failure to prevent masking the original error
    }
}
