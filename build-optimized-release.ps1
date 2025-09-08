# UPDATE
# Get the directory of the currently running script to build robust paths
$scriptRoot = Split-Path -Parent -Path $MyInvocation.MyCommand.Definition
$publishDir = Join-Path $scriptRoot "build\production"

# Clean up previous build artifacts
if (Test-Path $publishDir) {
    Write-Host "Removing previous build directory: $publishDir"
    Remove-Item -Recurse -Force $publishDir
}

while ($true) {
    Write-Host "Building and publishing Cliptoo.UI for production (Release configuration)..."
    
    # Publish with optimizations for a portable release:
    # --self-contained: Includes the .NET runtime, so no installation is needed.
    # -p:PublishSingleFile=true: Bundles everything into a single .exe file.
    # -p:PublishReadyToRun=true: Compiles to native code for faster startup.
    dotnet publish (Join-Path $scriptRoot "Cliptoo.UI\Cliptoo.UI.csproj") `
        -c Release `
        -o $publishDir `
        --self-contained true `
        -p:PublishSingleFile=true `
        -p:PublishReadyToRun=true

    if ($LASTEXITCODE -eq 0) {
        Write-Host "Publish successful. Output is in: $publishDir"
        break # Exit the loop on success
    }

    # Handle failure
    Write-Host "PUBLISH FAILED. Please review the errors above."
    $response = Read-Host "Do you want to try again? (y/n)"
    if ($response.ToLower() -ne 'y') {
        Write-Host "Exiting build process."
        pause
        exit 1 # Exit the script
    }
}

$exePath = Join-Path $publishDir "Cliptoo.UI.exe"

Write-Host "Checking for executable at: $exePath"

if (-not (Test-Path $exePath)) {
    Write-Host "RUN FAILED: Could not find the executable at the expected location."
    pause
    exit 1
}

Write-Host "Executable found. Launching Cliptoo..."
Start-Process -FilePath $exePath

Write-Host "Cliptoo has been launched."
pause