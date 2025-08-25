# Get the directory of the currently running script to build robust paths
$scriptRoot = Split-Path -Parent -Path $MyInvocation.MyCommand.Definition

Write-Host "Building and publishing Cliptoo.UI for production (Release configuration)..."
# Publish the UI project directly to ensure the RuntimeIdentifier is correctly used.
dotnet publish (Join-Path $scriptRoot "Cliptoo.UI\Cliptoo.UI.csproj") -c Release

if ($LASTEXITCODE -ne 0) {
    Write-Host "PUBLISH FAILED. Please review the errors above."
    pause
    exit 1
}

Write-Host "Publish successful."

$exePath = Join-Path $scriptRoot "Cliptoo.UI\bin\Release\net9.0-windows\win-x64\publish\Cliptoo.UI.exe"

Write-Host "Checking for executable at: $exePath"

if (-not (Test-Path $exePath)) {
    Write-Host "RUN FAILED: Could not find the executable at the expected location."
    Write-Host "Please check the build output path in '.\Cliptoo.UI\bin\Release'"
    pause
    exit 1
}

Write-Host "Executable found. Launching Cliptoo..."
Start-Process -FilePath $exePath

Write-Host "Cliptoo has been launched."
pause