# List outdated NuGet packages and updates
function Get-OutdatedPackages {
    Write-Host "Ensuring packages are restored first..."
    if (-not (Invoke-DotnetCommand -Command "restore" -Arguments "`"$global:solutionFile`"")) {
        Write-Host "ERROR! Package restore failed." -ForegroundColor Red
        Invoke-Item (Get-LogFile)
        return
    }

    Write-Host "Checking for outdated NuGet packages..."
    if (Invoke-DotnetCommand -Command "list" -Arguments "`"$global:solutionFile`" package --outdated") {
        Write-Host "Check complete. See log." -ForegroundColor Green
        Invoke-Item (Get-LogFile)
    }
    else {
        Write-Host "ERROR! Packages check has failed. See log." -ForegroundColor Red
        Invoke-Item (Get-LogFile)
    }
}
# .NET Builder Toolbox for Cliptoo
# Clean, build, and manage the Cliptoo solution.
# -----------------------------------------------

# --- Global Variables ---
# Project-specific settings to be configured.
$global:appName = "Cliptoo.UI"
$global:processNameForTermination = "Cliptoo.UI"
$global:solutionRoot = (Get-Item $PSScriptRoot).Parent.FullName
$global:solutionFile = Join-Path $solutionRoot "Cliptoo.sln"
$global:mainProjectFile = Join-Path $solutionRoot "Cliptoo.UI\Cliptoo.UI.csproj"
$global:requiredDotNetVersion = "9" # Major version number
$global:publishRuntimeId = "win-x64" # Used for portable package publish

# Velopack settings (for future integration)
$global:packageId = "Cliptoo" # Velopack package ID
$global:packageTitle = "Cliptoo"
$global:packageAuthors = "dcog989"
$global:packageIconPath = Join-Path $solutionRoot "Cliptoo.UI/Assets/Icons/cliptoo.ico"
$global:mainExeName = "Cliptoo.UI.exe"

# Post-build customization
$global:runPostBuildCleanup = $false # Cliptoo does not require special plugin cleanup
$global:removeCreateDump = $true # Remove createdump.exe from deps.json
$global:removeXmlFiles = $true # Remove *.xml doc files from output

# 7-Zip
$global:sevenZipPath = Join-Path $PSScriptRoot "7z/7za.exe"

# Menu item definitions for layout and logging
$global:menuItems = @{
    "1" = "Build & Run (Debug)"; "A" = "Open Output Folder"
    "2" = "Build & Run (Release)"; "B" = "Open Solution in IDE"
    "3" = "Watch & Run (Hot Reload)"; "C" = "Clean Solution"
    "4" = "Publish Portable Package"; "D" = "Clean Logs"
    "5" = "Publish Production Package"; "E" = "Change Version Number"
    "6" = "Restore NuGet Packages"; "F" = "Open User Data Folder"
    "7" = "List Packages + Updates"; "G" = "Open Log File"    
    "Q" = "Quit"
}
$global:logFile = $null
$global:sdkVersion = "N/A"
$global:gitBranch = ""
$global:gitCommit = ""


# --- Logging ---
function Get-LogFile {
    if ($null -eq $global:logFile) {
        $global:logFile = Start-Logging
    }
    return $global:logFile
}


function Start-Logging {
    $logDir = Join-Path $PSScriptRoot "Logs"
    if (-not (Test-Path $logDir)) {
        New-Item -ItemType Directory -Path $logDir | Out-Null
    }
    $logFile = Join-Path $logDir "Cliptoo.build.$((Get-Date).ToString('yyyyMMdd.HHmmss')).log"

    $appVersion = Get-BuildVersion
    if ([string]::IsNullOrEmpty($appVersion)) { $appVersion = "N/A" }

    # Create a transcript header
    $header = @"
***************************************
$($global:packageTitle), version = $appVersion
Log Start: $((Get-Date).ToString('yyyyMMddHHmmss'))
Username:   $($env:USERDOMAIN)\$($env:USERNAME)
***************************************
"@
    Set-Content -Path $logFile -Value $header

    return $logFile
}


function Clear-Logs {
    Write-Host "Cleaning logs..." -ForegroundColor Yellow
    $logDir = Join-Path $PSScriptRoot "Logs"
    if (Test-Path $logDir) {
        $logFiles = Get-ChildItem -Path $logDir -Filter "*.log"
        if ($null -ne $global:logFile) {
            # Exclude the current session's log file from deletion
            $logFiles = $logFiles | Where-Object { $_.FullName -ne $global:logFile }
        }

        if ($logFiles) {
            Write-Host "Removing $($logFiles.Count) old log files from $logDir"
            Remove-Item -Path $logFiles.FullName -Force -ErrorAction SilentlyContinue
        }
        else {
            Write-Host "No old log files to clean."
        }
    }
    else {
        Write-Host "Log directory not found."
    }
    Write-Host "Log cleanup complete." -ForegroundColor Green
}


# --- Prerequisite Check ---
function Test-DotNetVersion {
    Write-Host "Checking for .NET $($global:requiredDotNetVersion) SDK..." -NoNewline
    if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
        $msg = " 'dotnet.exe' not found. Ensure .NET $($global:requiredDotNetVersion) SDK is installed and in your PATH."
        Write-Host $msg -ForegroundColor Red
        return [PSCustomObject]@{ Success = $false; Message = $msg }
    }

    $version_output = (dotnet --version)
    if ($version_output -match "^$($global:requiredDotNetVersion)\.") {
        Write-Host " Found version $version_output." -ForegroundColor Green
        return [PSCustomObject]@{ Success = $true; Message = "Found version $version_output." }
    }
    else {
        $msg = " Required version $($global:requiredDotNetVersion).*, but found $version_output. Please install the correct SDK."
        Write-Host $msg -ForegroundColor Red
        return [PSCustomObject]@{ Success = $false; Message = $msg }
    }
}


# --- Command Execution Helper ---
function Invoke-DotnetCommand {
    param(
        [string]$Command,
        [string]$Arguments,
        [switch]$IgnoreErrors
    )
    $invokeParams = @{
        ExecutablePath = "dotnet"
        Arguments      = "$Command $Arguments --verbosity normal"
    }
    if ($IgnoreErrors) {
        $invokeParams.Add("IgnoreErrors", $true)
    }
    Invoke-ExternalCommand @invokeParams
}


function Invoke-ExternalCommand {
    param(
        [string]$ExecutablePath,
        [string]$Arguments,
        [string]$WorkingDirectory = "",
        [switch]$IgnoreErrors
    )
    $logFile = Get-LogFile
    $processInfo = New-Object System.Diagnostics.ProcessStartInfo
    $processInfo.FileName = $ExecutablePath
    $processInfo.Arguments = $Arguments
    $processInfo.RedirectStandardOutput = $true
    $processInfo.RedirectStandardError = $true
    $processInfo.UseShellExecute = $false
    $processInfo.CreateNoWindow = $true
    if (-not [string]::IsNullOrEmpty($WorkingDirectory)) {
        $processInfo.WorkingDirectory = $WorkingDirectory
    }

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $processInfo
    
    $stdOutHandler = { if (-not [string]::IsNullOrEmpty($EventArgs.Data)) { $EventArgs.Data | Out-File -FilePath $logFile -Append } }
    $stdErrHandler = { if (-not [string]::IsNullOrEmpty($EventArgs.Data)) { "ERROR: $($EventArgs.Data)" | Out-File -FilePath $logFile -Append } }
    
    $stdOutEvent = Register-ObjectEvent -InputObject $process -EventName "OutputDataReceived" -Action $stdOutHandler
    $stdErrEvent = Register-ObjectEvent -InputObject $process -EventName "ErrorDataReceived" -Action $stdErrHandler

    try {
        $process.Start() | Out-Null
    }
    catch {
        Write-Host "Error starting process '$ExecutablePath': $_" -ForegroundColor Red
        return $false
    }
    
    $process.BeginOutputReadLine()
    $process.BeginErrorReadLine()

    while (-not $process.HasExited) {
        Write-Host "." -NoNewline
        Start-Sleep -Seconds 1
    }
    $process.WaitForExit()

    # Always write a newline after the progress dots to clean up the console line.
    Write-Host ""

    Unregister-Event -SubscriptionId $stdOutEvent.Id
    Unregister-Event -SubscriptionId $stdErrEvent.Id
    
    return ($process.ExitCode -eq 0 -or $IgnoreErrors)
}

# --- Helpers ---
function Get-BuildVersion {
    if (-not (Test-Path $global:mainProjectFile)) {
        Write-Host "Warning: Main project file not found at '$($global:mainProjectFile)'. Version cannot be determined." -ForegroundColor Yellow
        return $null
    }
    try {
        $csprojContent = [xml](Get-Content $global:mainProjectFile)
        # Select the first PropertyGroup, which is typically the unconditional one containing the version.
        $version = $csprojContent.Project.PropertyGroup[0].Version
        if ($null -ne $version) {
            return $version.Trim()
        }
        # If the first PropertyGroup doesn't have a version, search all of them.
        $versionNode = $csprojContent.Project.PropertyGroup | Where-Object { $_.Version } | Select-Object -First 1
        if ($null -ne $versionNode) {
            return $versionNode.Version.Trim()
        }
        return $null # Return null if no version tag found anywhere
    }
    catch {
        Write-Host "Error parsing version from '$($global:mainProjectFile)': $_" -ForegroundColor Red
        return $null
    }
}


function Confirm-ProcessTermination {
    param([string]$Action = "Build")
    $process = @(Get-Process -Name $global:processNameForTermination -ErrorAction SilentlyContinue)
    if ($process.Count -gt 0) {
        $pids = $process.Id -join ', '
        Write-Host "$($global:processNameForTermination) is running (PID(s): $pids)." -ForegroundColor Yellow
        $kill = Read-Host "Do you want to terminate it? (y/n)"
        if ($kill.ToLower() -eq 'y') {
            Stop-Process -Name $global:processNameForTermination -Force
            Write-Host "$($global:processNameForTermination) terminated."
            Start-Sleep -Seconds 1
        }
        else {
            Write-Host "$($Action) aborted." -ForegroundColor Red
            return $false
        }
    }
    return $true
}


function Invoke-BuildAndStage {
    param(
        [string]$PublishDir,
        [bool]$IsSelfContained
    )
    
    Remove-BuildOutput -NoConfirm
    
    if (Test-Path $PublishDir) {
        Remove-Item $PublishDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    $arguments = "publish `"$($global:mainProjectFile)`" -c Release -r $($global:publishRuntimeId) --self-contained $IsSelfContained -o `"$PublishDir`""
    if (-not (Invoke-DotnetCommand -Command "" -Arguments $arguments)) {
        Write-Host "ERROR! Publish failed." -ForegroundColor Red
        Invoke-Item (Get-LogFile)
        return $null
    }

    return $PublishDir
}


function Invoke-ItemSafely {
    param(
        [string]$Path,
        [string]$ItemType = "Item"
    )
    if (-not (Test-Path $Path)) {
        Write-Host "Error: Could not find $ItemType at '$Path'." -ForegroundColor Red
        Read-Host "Press ENTER to continue..."
    }
    else {
        Invoke-Item $Path -ErrorAction SilentlyContinue
    }
}

function New-ChangelogFromGit {
    param([string]$OutputDir)
    Write-Host "Generating changelog from Git history..."
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Host "Warning: git.exe not found. Skipping changelog generation." -ForegroundColor Yellow
        return
    }

    try {
        $latestTag = (git -C $global:solutionRoot describe --tags --abbrev=0 2>$null).Trim()
        if ([string]::IsNullOrEmpty($latestTag)) { throw "No tags found." }
        Write-Host "Found latest tag: '$latestTag'. Generating changelog from new commits."
        $commitRange = "$latestTag..HEAD"
        $header = "## Changes since $latestTag"
    }
    catch {
        Write-Host "No Git tags found. Generating changelog for all commits." -ForegroundColor Yellow
        $commitRange = "HEAD"
        $header = "## Full Project Changelog"
    }

    try {
        $gitLogCommand = "log $commitRange --pretty=format:'* %h - %s (%an)'"
        $changelogContent = (git -C $global:solutionRoot $gitLogCommand)

        if ([string]::IsNullOrWhiteSpace(($changelogContent -join ''))) {
            Write-Host "No new commits to add to changelog. File not created." -ForegroundColor Yellow
            return
        }

        $fullContent = "$header`n`n$($changelogContent -join "`n")"
        $outputPath = Join-Path $OutputDir "Changelog.md"
        Set-Content -Path $outputPath -Value $fullContent
        Write-Host "Changelog.md created successfully." -ForegroundColor Green
    }
    catch {
        Write-Host "Warning: Failed to generate changelog from Git history. $_" -ForegroundColor Yellow
    }
}


# --- Core Functions (Menu Actions) ---
function Start-BuildAndRun {
    param($Configuration)
    if (-not (Confirm-ProcessTermination)) { return }

    Write-Host "Building solution in $Configuration mode..."
    if (-not (Invoke-DotnetCommand -Command "build" -Arguments "`"$solutionFile`" -c $Configuration")) {
        Write-Host "Build failed. See $(Get-LogFile) for details." -ForegroundColor Red
        Invoke-Item (Get-LogFile)
        return
    }

    Write-Host "Build successful. Starting application..."
    $exePath = Join-Path $solutionRoot "Cliptoo.UI\bin\x64\$Configuration\net9.0-windows\$($global:publishRuntimeId)\$($global:appName).exe"
    if (Test-Path $exePath) {
        Start-Process $exePath
        Write-Host "Application started." -ForegroundColor Green
    }
    else {
        Write-Host "ERROR! Main application EXE not found at $exePath" -ForegroundColor Red
    }
}


function Watch-And-Run {
    if (-not (Confirm-ProcessTermination)) { return }
    Write-Host "Starting dotnet watch. Press CTRL+C in the new window to stop."
    $arguments = "watch --project `"$($global:mainProjectFile)`" run"
    "dotnet $arguments" | Out-File -FilePath (Get-LogFile) -Append
    Start-Process "dotnet" -ArgumentList $arguments
}


function Restore-NuGetPackages {
    Write-Host "Restoring NuGet packages..."
    if (Invoke-DotnetCommand -Command "restore" -Arguments "`"$global:solutionFile`"") {
        Write-Host "NuGet packages restored successfully." -ForegroundColor Green
    }
    else {
        Write-Host "ERROR! NuGet packages not restored." -ForegroundColor Red
        Invoke-Item (Get-LogFile)
    }
}


function Publish-Portable {
    if (-not (Confirm-ProcessTermination -Action "Publish")) { return }

    $publishDir = Join-Path $solutionRoot "Output\Portable_Staging"
    $publishDir = Invoke-BuildAndStage -PublishDir $publishDir -IsSelfContained $true
    if ($null -eq $publishDir) { return }

    Write-Host "Post-build processing..."
    if ($global:removeCreateDump) {
        Remove-CreateDumpReference -Path $publishDir
    }
    if ($global:removeXmlFiles) {
        Write-Host "Removing documentation files (*.xml)..."
        Get-ChildItem -Path $publishDir -Include "*.xml" -Recurse | Remove-Item -Force
    }

    Write-Host "Adding portable mode marker..."
    $portableMarkerPath = Join-Path $publishDir "cliptoo.portable"
    Set-Content -Path $portableMarkerPath -Value "This file enables portable mode. Do not delete."

    Write-Host "Removing debug symbols (*.pdb)..."
    Get-ChildItem -Path $publishDir -Include "*.pdb" -Recurse | Remove-Item -Force

    Write-Host "Archiving portable package..."
    if (-not (Test-Path $global:sevenZipPath)) {
        Write-Host "ERROR! 7za.exe not found at $($global:sevenZipPath)"; Invoke-Item (Get-LogFile); return
    }
    
    $finalOutputDir = Join-Path $solutionRoot "Output"
    if (-not (Test-Path $finalOutputDir)) { New-Item $finalOutputDir -ItemType Directory -Force | Out-Null }
    $destinationArchive = Join-Path $finalOutputDir "$($global:packageTitle)-Portable.7z"
    if (Test-Path $destinationArchive) { Remove-Item $destinationArchive -Force }
    
    $sourceDir = Join-Path $publishDir "*"
    $7zArgs = "a -t7z -m0=lzma2 -mx=3 `"$destinationArchive`" `"$sourceDir`""
    
    if (-not (Invoke-ExternalCommand -ExecutablePath $global:sevenZipPath -Arguments $7zArgs)) {
        Write-Host "ERROR! 7-Zip archiving failed."; Invoke-Item (Get-LogFile); return
    }
    
    Remove-Item $publishDir -Recurse -Force
    New-ChangelogFromGit -OutputDir $finalOutputDir

    Write-Host "Portable archive created: $destinationArchive" -ForegroundColor Green
    Invoke-Item $finalOutputDir
}


# Publish a full release: standard, portable, and changelog
function Build-ProductionPackage {
    $releaseDir = Join-Path $global:solutionRoot "release"
    if (Test-Path $releaseDir) {
        Remove-Item -Recurse -Force $releaseDir
    }
    New-Item -ItemType Directory -Path $releaseDir | Out-Null

    $buildOutputDir = Join-Path $global:solutionRoot "bin\x64"

    # Build Standard Release
    $stdPublishDir = Join-Path $buildOutputDir "standard"
    if (Test-Path $stdPublishDir) { Remove-Item -Recurse -Force $stdPublishDir }
    $publishArgs = "`"$global:mainProjectFile`" -c Release -o `"$stdPublishDir`" --self-contained true -p:PublishSingleFile=true -p:PublishReadyToRun=true"
    if (-not (Invoke-DotnetCommand -Command "publish" -Arguments $publishArgs)) {
        Write-Host "Standard release build failed." -ForegroundColor Red
        return
    }
    Get-ChildItem -Path $stdPublishDir -Filter "*.pdb" -Recurse | Remove-Item -Force

    # Archive Standard Release
    $version = Get-BuildVersion
    $stdArchiveName = "Cliptoo-Windows-x64-v$version.7z"
    $stdArchivePath = Join-Path $buildOutputDir $stdArchiveName
    $sevenZipPath = Join-Path $PSScriptRoot "7z/7za.exe"
    if (Test-Path $sevenZipPath) {
        & $sevenZipPath a -t7z -mx=3 "$stdArchivePath" "$stdPublishDir\*" | Out-Null
    }
    else {
        Compress-Archive -Path "$stdPublishDir\*" -DestinationPath $stdArchivePath -Force
    }

    # Build Portable Release
    $portablePublishDir = Join-Path $buildOutputDir "portable"
    if (Test-Path $portablePublishDir) { Remove-Item -Recurse -Force $portablePublishDir }
    $publishArgs = "`"$global:mainProjectFile`" -c Release -o `"$portablePublishDir`" --self-contained true -p:PublishSingleFile=true -p:PublishReadyToRun=true"
    if (-not (Invoke-DotnetCommand -Command "publish" -Arguments $publishArgs)) {
        Write-Host "Portable release build failed." -ForegroundColor Red
        return
    }
    Get-ChildItem -Path $portablePublishDir -Filter "*.pdb" -Recurse | Remove-Item -Force
    $portableMarkerPath = Join-Path $portablePublishDir "cliptoo.portable"
    Set-Content -Path $portableMarkerPath -Value "This is a critical file. Do not delete."

    # Archive Portable Release
    $portableArchiveName = "Cliptoo-Windows-x64-Portable-v$version.7z"
    $portableArchivePath = Join-Path $buildOutputDir $portableArchiveName
    if (Test-Path $sevenZipPath) {
        & $sevenZipPath a -t7z -mx=3 "$portableArchivePath" "$portablePublishDir\*" | Out-Null
    }
    else {
        Compress-Archive -Path "$portablePublishDir\*" -DestinationPath $portableArchivePath -Force
    }

    # Copy archives to release dir
    Get-ChildItem -Path $buildOutputDir -File | Where-Object { $_.Extension -in ".zip", ".7z" } | Copy-Item -Destination $releaseDir

    # Generate Changelog
    if (Get-Command git -ErrorAction SilentlyContinue) {
        Write-Host "Generating CHANGELOG.md from git history..."
        $changelogOutputPath = Join-Path $releaseDir "CHANGELOG.md"
        try {
            $latestTag = git -C $global:solutionRoot describe --tags --abbrev=0 2>$null
            $gitLogCommand = if ($latestTag) { "git -C $global:solutionRoot log $latestTag..HEAD --pretty=format:'- %s (%h)'" } else { "git -C $global:solutionRoot log --pretty=format:'- %s (%h)'" }
            $logEntries = Invoke-Expression $gitLogCommand
            $changelogHeader = "# Cliptoo Changelog`n`n"
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


function Remove-BuildOutput {
    param([switch]$NoConfirm)
    if (-not $NoConfirm -and -not (Confirm-ProcessTermination -Action "Clean")) {
        return
    }

    Write-Host "Cleaning build files..." -ForegroundColor Yellow
    $outputDir = Join-Path $solutionRoot "Output"
    if (Test-Path $outputDir) {
        Write-Host "Removing $outputDir"
        Remove-Item -Recurse -Force $outputDir
    }
    Get-ChildItem -Path $solutionRoot -Include bin, obj -Recurse -Directory -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Cleanup complete." -ForegroundColor Green
}


function Update-VersionNumber {
    if (-not(Test-Path $global:mainProjectFile)) {
        Write-Host "ERROR! Project file not found at: $($global:mainProjectFile)" -ForegroundColor Red
        return
    }
    
    $currentVersion = Get-BuildVersion
    Write-Host "Current version: $currentVersion" -ForegroundColor Yellow

    $newVersion = Read-Host "Enter new version, or 'X' to cancel"
    if ([string]::IsNullOrWhiteSpace($newVersion) -or $newVersion.ToLower() -eq 'x') {
        Write-Host "Operation cancelled."; return
    }

    try {
        if ($newVersion -notmatch '^\d+\.\d+\.\d+') {
            throw "Invalid version format."
        }
        
        $csproj = [xml](Get-Content $global:mainProjectFile)
        # Target the first PropertyGroup which is standard for the <Version> tag.
        $propertyGroupToUpdate = $csproj.Project.PropertyGroup[0]

        if ($null -ne $propertyGroupToUpdate.Version) {
            $propertyGroupToUpdate.Version = $newVersion
        }
        else {
            # If <Version> doesn't exist in the first group, create it.
            $versionElement = $csproj.CreateElement("Version")
            $versionElement.InnerText = $newVersion
            $propertyGroupToUpdate.AppendChild($versionElement) | Out-Null
        }
        
        $csproj.Save($global:mainProjectFile)
        Write-Host "Version updated to $newVersion in $($global:mainProjectFile)" -ForegroundColor Green
        "Version updated from '$currentVersion' to '$newVersion' in $($global:mainProjectFile)." | Out-File -FilePath (Get-LogFile) -Append
    }
    catch {
        Write-Host "ERROR! Invalid version. Use Semantic Versioning - Major.Minor.Patch - e.g., `1.2.4`." -ForegroundColor Red
    }
}


function Open-LatestLogFile {
    $logDir = Join-Path $PSScriptRoot "Logs"
    $latestLog = Get-ChildItem -Path $logDir -Filter "*.log" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($latestLog) {
        Invoke-ItemSafely -Path $latestLog.FullName -ItemType "Log file"
    }
    else {
        Write-Host "No logs found to open." -ForegroundColor Yellow
        Start-Sleep -Seconds 2
    }
}

function Open-UserDataFolder {
    $exePath = Join-Path $global:solutionRoot "Cliptoo.UI\bin\Release\net9.0-windows\$($global:publishRuntimeId)"
    $portableMarkerPath = Join-Path $exePath "cliptoo.portable"
    
    $userDataPath = ""
    if (Test-Path $portableMarkerPath) {
        # Portable mode path
        $userDataPath = Join-Path $exePath "Data"
    }
    else {
        # Standard mode path
        $userDataPath = Join-Path $env:APPDATA 'Cliptoo'
    }
    Invoke-ItemSafely -Path $userDataPath -ItemType "User data folder"
}

# --- Post-Build Functions ---
function Remove-CreateDumpReference {
    param($Path)
    Write-Host "Removing createdump reference..." -ForegroundColor Cyan
    $depjsonPath = Join-Path $Path "$($global:appName).deps.json"
    if (Test-Path $depjsonPath) {
        $depjson = Get-Content $depjsonPath -raw
        $depjson -replace '(?s)(.createdump.exe": {.*?}.*?\n)\s*', "" | Out-File $depjsonPath -Encoding UTF8
    }
    Remove-Item -Path (Join-Path $Path "createdump.exe") -ErrorAction SilentlyContinue
}

# --- Menu Display ---
function Show-Menu {
    param([string]$LogFile)
    
    $appVersion = Get-BuildVersion
    if ([string]::IsNullOrEmpty($appVersion)) { $appVersion = "N/A (check $($global:mainProjectFile))" }
    
    $logFileName = "Not yet created."
    if (-not [string]::IsNullOrEmpty($LogFile)) {
        $logFileName = Split-Path $LogFile -Leaf
    }
    
    Write-Host "-----------------------------------------------------------------------" -ForegroundColor Green
    Write-Host "              .NET Builder Toolbox for $($global:packageTitle)" -ForegroundColor Green
    Write-Host "-----------------------------------------------------------------------" -ForegroundColor Green
    Write-Host "Solution: $($global:solutionFile)" -ForegroundColor DarkGray
    if (-not [string]::IsNullOrEmpty($global:gitBranch)) {
        Write-Host "Branch:   $($global:gitBranch) ($($global:gitCommit))" -ForegroundColor DarkGray
    }
    Write-Host "Version:  $appVersion" -ForegroundColor DarkGray
    Write-Host "SDK:      $($global:sdkVersion)" -ForegroundColor DarkGray
    Write-Host "Logging:  $logFileName" -ForegroundColor DarkGray
    Write-Host "-----------------------------------------------------------------------" -ForegroundColor Green

    $menuTable = @(
        [PSCustomObject]@{ Left = "1. $($global:menuItems['1'])"; Right = "A. $($global:menuItems['A'])" }
        [PSCustomObject]@{ Left = "2. $($global:menuItems['2'])"; Right = "B. $($global:menuItems['B'])" }
        [PSCustomObject]@{ Left = "3. $($global:menuItems['3'])"; Right = "C. $($global:menuItems['C'])" }
        [PSCustomObject]@{ Left = "4. $($global:menuItems['4'])"; Right = "D. $($global:menuItems['D'])" }
        [PSCustomObject]@{ Left = "5. $($global:menuItems['5'])"; Right = "E. $($global:menuItems['E'])" }
        [PSCustomObject]@{ Left = "6. $($global:menuItems['6'])"; Right = "F. $($global:menuItems['F'])" }
        [PSCustomObject]@{ Left = "7. $($global:menuItems['7'])"; Right = "G. $($global:menuItems['G'])" }
    )
    ($menuTable | Format-Table -HideTableHeaders -AutoSize | Out-String).Trim() | Write-Host
    
    Write-Host "Q. $($global:menuItems['Q'])" -ForegroundColor Magenta
    Write-Host "-----------------------------------------------------------------------" -ForegroundColor Green
    Write-Host "Run option:" -ForegroundColor Cyan -NoNewline
}


# --- Main Execution Logic ---
if (-not (Test-Path $global:solutionFile)) {
    Write-Host "ERROR! Solution file not found at '$($global:solutionFile)'. Ensure this script is in the correct project directory." -ForegroundColor Red
    Read-Host "ENTER to exit..."
    return
}

try {
    # --- Pre-run Info Gathering ---
    $global:sdkVersion = (dotnet --version 2>$null).Trim()
    if (Get-Command git -ErrorAction SilentlyContinue) {
        $global:gitBranch = (git -C $global:solutionRoot rev-parse --abbrev-ref HEAD 2>$null).Trim()
        $global:gitCommit = (git -C $global:solutionRoot rev-parse --short HEAD 2>$null).Trim()
    }

    $dotNetCheckResult = Test-DotNetVersion
    if (-not $dotNetCheckResult.Success) {
        $logFile = Get-LogFile # Create log ONLY on failure
        "Prerequisite check failed: $($dotNetCheckResult.Message)" | Out-File -FilePath $logFile -Append
        Invoke-Item $logFile
        Read-Host "Prerequisite check failed. ENTER to exit..."
        return
    }

    $exit = $false
    while (-not $exit) {
        Clear-Host
        try { [System.Console]::SetCursorPosition(0, 0) } catch {}
        Show-Menu -LogFile $global:logFile
        $choice = Read-Host

        if ($choice.ToLower() -ne 'q') {
            $logFile = Get-LogFile
            $choiceKey = $choice.ToUpper()
            if ($global:menuItems.ContainsKey($choiceKey)) {
                $description = $global:menuItems[$choiceKey]
                "User selected option: '$choice' ($description)" | Out-File -FilePath $logFile -Append
            }
            else {
                "User selected invalid option: '$choice'" | Out-File -FilePath $logFile -Append
            }
        }

        switch ($choice.ToLower()) {
            "1" { Start-BuildAndRun -Configuration "Debug"; Read-Host "ENTER to continue..." }
            "2" { Start-BuildAndRun -Configuration "Release"; Read-Host "ENTER to continue..." }
            "3" { Watch-And-Run }
            "4" { Publish-Portable; Read-Host "ENTER to continue..." }
            "5" { Build-ProductionPackage; Read-Host "ENTER to continue..." }
            "6" { Restore-NuGetPackages; Start-Sleep -Seconds 2 }
            "7" { Get-OutdatedPackages; Read-Host "ENTER to continue..." }

            "a" { Invoke-ItemSafely -Path (Join-Path $solutionRoot "Cliptoo.UI/bin") -ItemType "Output folder" }
            "b" { Invoke-ItemSafely -Path $solutionFile -ItemType "Solution file" }
            "c" { Remove-BuildOutput; Start-Sleep -Seconds 2 }
            "d" { Clear-Logs; Start-Sleep -Seconds 2 }
            "e" { Update-VersionNumber; Start-Sleep -Seconds 2 }
            "f" { Open-UserDataFolder }
            "g" { Open-LatestLogFile }

            "q" { $exit = $true }
            default { Write-Host "Invalid option." -ForegroundColor Red; Start-Sleep -Seconds 2 }
        }
    }
}
catch {
    $logFile = Get-LogFile
    $errorMsg = "A script-terminating error occurred.`nERROR: $($_.Exception.Message)`n$($_ | Format-List * -Force | Out-String)"
    $errorMsg | Out-File -FilePath $logFile -Append
    Write-Host "ERROR! Something went wrong. See log for details: $logFile" -ForegroundColor Red
    Invoke-Item $logFile
    Read-Host "ENTER to exit..."
}
finally {
    Write-Host "Exiting script."
}