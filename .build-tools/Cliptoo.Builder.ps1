# Get the directory of the currently running script to build robust paths
$scriptRoot = Split-Path -Parent -Path $MyInvocation.MyCommand.Definition
# Force Resolve-Path to return a single string to prevent array-related errors
$solutionRoot = (Resolve-Path (Join-Path $scriptRoot "..")) | Select-Object -First 1

# --- Setup Logging ---
$logTimestamp = Get-Date -Format "yyyyMMdd.HHmmss"
$logFileName = "Cliptoo.Builder.$logTimestamp.log"
$logFilePath = Join-Path $scriptRoot $logFileName
Start-Transcript -Path $logFilePath -Append

# --- Dependency Checks ---
$sevenZipPath = Join-Path $scriptRoot "7z\7za.exe"
$hasSevenZip = Test-Path $sevenZipPath
if ($hasSevenZip) {
    Write-Host "7za.exe found, ready for fast archiving." -ForegroundColor Green
}
else {
    Write-Warning "7za.exe not found at '$sevenZipPath'. Falling back to slower Compress-Archive."
}

$hasGit = $false
if (Get-Command git -ErrorAction SilentlyContinue) {
    $hasGit = $true
}
else {
    Write-Warning "git.exe not found in PATH. Changelog generation will be skipped."
}


# --- Functions ---
function Assert-NoRunningIDEs {
    $ideProcesses = @(
        @{ Name = "Visual Studio"; ProcessName = "devenv" },
        @{ Name = "Visual Studio Code"; ProcessName = "Code" },
        @{ Name = "JetBrains Rider"; ProcessName = "rider64" },
        @{ Name = ".NET CLI Watcher"; ProcessName = "dotnet" }
    )
    $runningIDEs = [System.Collections.Generic.List[string]]::new()
    foreach ($ide in $ideProcesses) {
        if (Get-Process -Name $ide.ProcessName -ErrorAction SilentlyContinue) {
            $runningIDEs.Add($ide.Name)
        }
    }
    if ($runningIDEs.Count -eq 0) { return }
    
    $isCi = $env:CI -or $env:TF_BUILD -or $env:GITHUB_ACTIONS
    $ideList = $runningIDEs -join ', '
    Write-Warning "The following development apps / processes are active: $ideList."
    
    if (-not $isCi) {
        Write-Warning "Leaving these running may cause file lock errors."
        $choice = Read-Host "Do you want to continue anyway? [y/N]"
        if ($choice -notlike 'y*') {
            throw "User aborted due to running IDEs."
        }
    }
    else {
        Write-Host "CI environment detected. Proceeding..."
    }
}

function Invoke-Clean {
    Write-Host "--- Starting Clean Operation ---" -ForegroundColor Cyan
    try {
        Assert-NoRunningIDEs
        
        $slnPath = Join-Path $solutionRoot "Cliptoo.sln"
        if (Test-Path $slnPath) {
            # Use quiet verbosity and redirect output to ensure console is clean.
            # Start-Transcript will still capture the full command output in the log file.
            dotnet clean $slnPath --verbosity quiet 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) {
                Write-Warning "'dotnet clean' failed. Check log for details."
            }
        }
        else {
            Write-Warning "Solution file not found. Skipping dotnet clean."
        }

        # --- Manually remove all bin and obj directories for a deeper clean ---
        Write-Host "Performing deep clean of bin/obj directories..."
        $dirsToClean = Get-ChildItem -Path $solutionRoot -Recurse -Directory | Where-Object { $_.Name -in @('bin', 'obj') }
        foreach ($dir in $dirsToClean) {
            Remove-Item -Path $dir.FullName -Recurse -Force -ErrorAction SilentlyContinue
            if (Test-Path $dir.FullName) {
                Write-Warning "Could not fully remove directory '$($dir.FullName)'. It may be locked by another process."
            }
        }

        # --- Also remove the custom release directory ---
        $releaseDir = Join-Path $solutionRoot "release"
        if (Test-Path $releaseDir) {
            Remove-Item -Path $releaseDir -Recurse -Force -ErrorAction SilentlyContinue
            if (Test-Path $releaseDir) {
                Write-Warning "Could not fully remove directory '$releaseDir'. It may be locked by another process."
            }
        }
        Write-Host "--- Clean Operation Finished ---" -ForegroundColor Cyan
    }
    catch {
        Write-Error "An error occurred during cleanup: $($_.Exception.Message)"
    }
}

function Invoke-DotNetPublish {
    param(
        [string]$Arguments
    )
    $success = $false
    while ($true) {
        $csprojPath = Join-Path $solutionRoot 'Cliptoo.UI\Cliptoo.UI.csproj'
        $command = "dotnet publish `"$csprojPath`" $Arguments"

        $scriptBlock = {
            param($cmd)
            # Redirect ALL streams (*>&1) to ensure progress messages are captured and not printed live.
            $output = Invoke-Expression -Command $cmd *>&1 | Out-String
            return [pscustomobject]@{
                Output   = $output
                ExitCode = $LASTEXITCODE
            }
        }

        $job = Start-Job -ScriptBlock $scriptBlock -ArgumentList $command
        
        [System.Console]::Write("Build in progress")
        while ($job.State -eq 'Running') {
            [System.Console]::Write(".")
            Start-Sleep -Seconds 1
        }
        [System.Console]::WriteLine()

        $result = Receive-Job $job
        Remove-Job $job
        
        # Filter out the interactive "Removed..." lines from the output before printing.
        # This prevents console corruption artifacts while still logging warnings and errors.
        $filteredOutput = $result.Output.Split([System.Environment]::NewLine) | Where-Object {
            $_.Trim() -notmatch '^\s*Removed \d+ of \d+ files'
        }
        Write-Host ($filteredOutput -join [System.Environment]::NewLine)

        if ($result.ExitCode -eq 0) {
            $success = $true
            break
        }
        
        Write-Host "PUBLISH FAILED. Please review the errors in the log file." -ForegroundColor Red
        $response = Read-Host "Do you want to try again? (y/n)"
        if ($response.ToLower() -ne 'y') { break }
    }
    return $success
}

function Get-ProjectVersion {
    $csprojPath = Join-Path $solutionRoot "Cliptoo.UI\Cliptoo.UI.csproj"
    if (-not (Test-Path $csprojPath)) {
        Write-Warning "Could not find Cliptoo.UI.csproj to determine version."
        return "0.0.0"
    }
    try {
        $csprojContent = [xml](Get-Content $csprojPath)
        return ($csprojContent.Project.PropertyGroup.Version | Select-Object -First 1).Trim()
    }
    catch {
        Write-Warning "Failed to parse version from csproj file."
        return "0.0.0"
    }
}

function Invoke-ArchiveAndCompress {
    param(
        [string]$SourceDir,
        [string]$ArchiveBaseName,
        [string]$BaseOutputDir
    )
    $ext = if ($hasSevenZip) { ".7z" } else { ".zip" }
    $archivePath = Join-Path $BaseOutputDir "$ArchiveBaseName$ext"

    if ($hasSevenZip) {
        Write-Host "Creating 7z archive..."
        & $sevenZipPath a -t7z -mx=3 "$archivePath" "$SourceDir\*" | Out-Null
        if ($LASTEXITCODE -ne 0) {
            Write-Host "7-Zip archiving failed." -ForegroundColor Red
            return $null
        }
    }
    else {
        Write-Host "Creating zip archive..."
        Compress-Archive -Path "$SourceDir\*" -DestinationPath $archivePath -Force
    }
    return $archivePath
}

function Build-And-Run-Dev {
    $runningProcess = Get-Process -Name "Cliptoo.UI" -ErrorAction SilentlyContinue
    if ($runningProcess) {
        Write-Warning "Cliptoo is already running. It needs to be closed to build new and run."
        $response = Read-Host "Do you want to stop the running process and continue? (y/n)"
        if ($response.ToLower() -eq 'y') {
            Stop-Process -Name "Cliptoo.UI" -Force
            Start-Sleep -Seconds 1
        }
        else {
            Write-Host "Build aborted by user." -ForegroundColor Red
            return
        }
    }

    Write-Host "Building and publishing Cliptoo.UI for development (Release configuration)..."
    if (-not (Invoke-DotNetPublish -Arguments "-c Release -v m")) { return }
    
    $exePath = Join-Path $solutionRoot "Cliptoo.UI\bin\Release\net9.0-windows\win-x64\publish\Cliptoo.UI.exe"
    if (-not (Test-Path $exePath)) {
        Write-Host "RUN FAILED: Could not find the executable at the expected location." -ForegroundColor Red
        return
    }
    Write-Host "Launching Cliptoo..."
    Start-Process -FilePath $exePath
}

function Build-Optimized-Internal {
    param(
        [string]$PublishDir
    )
    if (Test-Path $PublishDir) {
        Remove-Item -Recurse -Force $PublishDir
    }
    
    $parentDir = Split-Path -Parent $PublishDir
    if (-not (Test-Path $parentDir)) {
        New-Item -ItemType Directory -Path $parentDir | Out-Null
    }
    
    Write-Host "Building and publishing Cliptoo.UI for production (Release configuration)..."
    $publishArgs = "-c Release -v m -o `"$PublishDir`" --self-contained true -p:PublishSingleFile=true -p:PublishReadyToRun=true"
    return Invoke-DotNetPublish -Arguments $publishArgs
}

function New-StandardReleasePackage {
    param(
        [string]$BaseDir,
        [switch]$NoOpen,
        [string]$Version
    )
    $publishDir = Join-Path $BaseDir "standard"
    Write-Host "Creating Standard (AppData) Release Package..."
    if (-not (Build-Optimized-Internal -PublishDir $publishDir)) { return $false }
    Get-ChildItem -Path $publishDir -Filter "*.pdb" -Recurse | Remove-Item -Force
    
    $baseName = "Cliptoo-Windows-x64-v$Version"
    $archivePath = Invoke-ArchiveAndCompress -SourceDir $publishDir -ArchiveBaseName $baseName -BaseOutputDir $BaseDir
    if ($archivePath) {
        Write-Host "Standard release package created at: $archivePath" -ForegroundColor Green
        if (-not $NoOpen) { Invoke-Item $BaseDir }
        return $true
    }
    return $false
}

function New-PortableReleasePackage {
    param(
        [string]$BaseDir,
        [switch]$NoOpen,
        [string]$Version
    )
    $publishDir = Join-Path $BaseDir "portable"
    Write-Host "Creating Portable Release Package..."
    if (-not (Build-Optimized-Internal -PublishDir $publishDir)) { return $false }

    Get-ChildItem -Path $publishDir -Filter "*.pdb" -Recurse | Remove-Item -Force
    $portableMarkerPath = Join-Path $publishDir "cliptoo.portable"
    Set-Content -Path $portableMarkerPath -Value "This is a critical file. Do not delete."
    
    $baseName = "Cliptoo-Windows-x64-Portable-v$Version"
    $archivePath = Invoke-ArchiveAndCompress -SourceDir $publishDir -ArchiveBaseName $baseName -BaseOutputDir $BaseDir
    if ($archivePath) {
        Write-Host "Portable release package created at: $archivePath" -ForegroundColor Green
        if (-not $NoOpen) { Invoke-Item $BaseDir }
        return $true
    }
    return $false
}

function Show-Installer-ComingSoon {
    Write-Host "Creating an installer package with Inno Setup is planned for a future update." -ForegroundColor Yellow
}

function New-FullReleasePackage {
    param(
        [string]$Version
    )
    $releaseDir = Join-Path $solutionRoot "release"
    if (Test-Path $releaseDir) {
        Remove-Item -Recurse -Force $releaseDir
    }
    New-Item -ItemType Directory -Path $releaseDir | Out-Null
    
    $buildOutputDir = Join-Path $solutionRoot "bin\x64"
    $stdSuccess = New-StandardReleasePackage -BaseDir $buildOutputDir -NoOpen -Version $Version
    $portSuccess = New-PortableReleasePackage -BaseDir $buildOutputDir -NoOpen -Version $Version

    if (-not ($stdSuccess -and $portSuccess)) {
        Write-Host "One or more package builds failed. The final release package will be incomplete." -ForegroundColor Red
        return
    }

    Write-Host "Copying packages to final release directory..."
    Get-ChildItem -Path $buildOutputDir -File | Where-Object { $_.Extension -in ".zip", ".7z" } | Copy-Item -Destination $releaseDir
    
    if ($hasGit) {
        Write-Host "Generating CHANGELOG.md from git history..."
        $changelogOutputPath = Join-Path $releaseDir "CHANGELOG.md"
        try {
            $latestTag = git describe --tags --abbrev=0 2>$null
            $gitLogCommand = if ($latestTag) { "git log $latestTag..HEAD --pretty=format:'- %s (%h)'" } else { "git log --pretty=format:'- %s (%h)'" }
            $logEntries = Invoke-Expression $gitLogCommand
            $changelogHeader = "# Changelog`n`n"
            $changelogContent = if ($logEntries) { $changelogHeader + ($logEntries -join "`n") } else { $changelogHeader + "No new changes since the last version." }
            Set-Content -Path $changelogOutputPath -Value $changelogContent
            Write-Host "CHANGELOG.md generated successfully." -ForegroundColor Green
        }
        catch {
            Write-Host "Failed to generate changelog from git history." -ForegroundColor Red
        }
    }

    Write-Host "Full release package created successfully at: $releaseDir" -ForegroundColor Green
    Invoke-Item $releaseDir
}

function Invoke-CleanLogs {
    Write-Host "--- Cleaning Build Logs ---" -ForegroundColor Cyan
    $currentLog = $logFilePath
    $logFiles = Get-ChildItem -Path $scriptRoot -Filter "Cliptoo.Builder.*.log" | Where-Object { $_.FullName -ne $currentLog }
    if ($logFiles.Count -eq 0) {
        Write-Host "No old build logs found to clean."
        return
    }

    $deletedCount = 0
    foreach ($file in $logFiles) {
        try {
            Remove-Item -Path $file.FullName -Force
            $deletedCount++
        }
        catch {
            Write-Warning "Could not delete file: $($file.Name). It may be locked."
        }
    }

    Write-Host "Deleted $deletedCount old log file(s)." -ForegroundColor Green
    Write-Host "--- Log Cleaning Finished ---" -ForegroundColor Cyan
}

# Main script logic
try {
    $projectVersion = Get-ProjectVersion
    :mainLoop while ($true) {
        try {
            # This combination is more robust for clearing the screen across different hosts.
            [System.Console]::SetCursorPosition(0, 0)
        }
        catch {
            # Silently ignore if the host doesn't support setting cursor position.
        }
        [System.Console]::Clear()
        Write-Host "Cliptoo Builder"
        Write-Host "--------------------------------------------------------"
        Write-Host "1) Build & Run (Development)"
        Write-Host "2) Create Standard Release Package"
        Write-Host "3) Create Portable Release Package"
        Write-Host "4) Create Installer (Coming Soon!)"
        Write-Host "5) Create Full Release (Standard + Portable + Changelog)"
        Write-Host "C) Clean Solution"
        Write-Host "CL) Clean Build Logs"
        Write-Host "Q) Quit"
        Write-Host "* add 'c' suffix to clean before building,  e.g. '1c'"
        Write-Host "--------------------------------------------------------"
        Write-Host "Cliptoo version = v$projectVersion"
        Write-Host " "
        
        $rawChoice = Read-Host -Prompt "Select an option"
        # Echo the user's choice to the host so it gets captured by the transcript log.
        Write-Host "User selection: '$rawChoice'"
        $choice = $rawChoice.Trim().ToLower()
        $cleanFirst = $false

        if ($choice.EndsWith('c') -and $choice.Length -gt 1) {
            $cleanFirst = $true
            $choice = $choice.TrimEnd('c')
        }
        
        if ($cleanFirst) {
            Invoke-Clean
        }
        
        $operationCompleted = $true
        switch ($choice) {
            "1" { Build-And-Run-Dev }
            "2" { [void](New-StandardReleasePackage -BaseDir (Join-Path $solutionRoot "bin\x64") -Version $projectVersion) }
            "3" { [void](New-PortableReleasePackage -BaseDir (Join-Path $solutionRoot "bin\x64") -Version $projectVersion) }
            "4" { Show-Installer-ComingSoon }
            "5" { New-FullReleasePackage -Version $projectVersion }
            "c" { if (-not $cleanFirst) { Invoke-Clean } } # Avoid double-clean
            "cl" { Invoke-CleanLogs }
            "q" { Write-Host "Exiting."; break mainLoop }
            default {
                Write-Host "Invalid option. Please try again." -ForegroundColor Yellow
                $operationCompleted = $false
                Start-Sleep -Seconds 2
            }
        }
        
        if ($operationCompleted) {
            Write-Host ("-" * 60)
            Write-Host "Operation complete. Press any key to return to the menu..." -NoNewline
            $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown") | Out-Null
        }
    }
}
finally {
    Write-Host "Build process finished. Log saved to '$logFilePath'."
    Stop-Transcript | Out-Null
}