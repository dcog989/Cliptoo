# Builder Toolbox for .NET Apps
# Clean, build, and manage .NET solutions.
# version: 1.15.0
# -----------------------------------------------------------------------------
# * Edit these settings...
# -----------------------------------------------------------------------------
$Script:PackageTitle = "Cliptoo"                    # user-facing name of the application.
$Script:MainProjectName = "Cliptoo.UI"              # name of the main .csproj file (without the extension).
$Script:SolutionFileName = "Cliptoo.sln"            # name of the .sln file.
$Script:SolutionSubFolder = ""                      # folder containing the .sln file, relative to repo root. Set to "" if .sln is at the root.
$Script:MainProjectSourcePath = "Cliptoo.UI"        # path from the solution folder to the main project's folder.
$Script:PackageAuthors = "dcog989"                  # author's name.
$Script:RequiredDotNetVersion = "9"                 # major version number.
$Script:TargetFramework = "net9.0-windows"          # from project `.csproj` file.
$Script:BuildPlatform = "x64"                       # target build platform (e.g., x64, AnyCPU).
$Script:PublishRuntimeId = "win-x64"                # target runtime for publishing.
$Script:UseVelopack = $true                         # use Velopack for production builds, $false to use 7-Zip.
$Script:VelopackChannelName = "prod"                # release channel (e.g., 'prod', 'staging').


# -----------------------------------------------------------------------------
# ! No edits past this point - hopefully.
# -----------------------------------------------------------------------------

# Core project settings
$Script:RepoRoot = (Get-Item $PSScriptRoot).Parent.FullName
$Script:SolutionRoot = if (-not [string]::IsNullOrEmpty($Script:SolutionSubFolder)) { Join-Path $Script:RepoRoot $Script:SolutionSubFolder } else { $Script:RepoRoot }
$Script:SolutionFile = Join-Path $Script:SolutionRoot $Script:SolutionFileName

$Script:MainProjectDir = Join-Path $Script:SolutionRoot $Script:MainProjectSourcePath
$Script:MainProjectFile = Join-Path $Script:MainProjectDir "$($Script:MainProjectName).csproj"

# Velopack settings
$Script:PackageId = $Script:PackageTitle
$Script:PackageIconPath = Join-Path $Script:MainProjectDir "Assets/Icons/$($Script:PackageTitle.ToLower()).ico"
$Script:MainExeName = "$($Script:MainProjectName).exe"
$Script:AppDataFolderName = $Script:PackageTitle

# Package naming and markers
$Script:PortableMarkerFile = "$($Script:PackageTitle.ToLower()).portable"
$Script:StandardArchiveFormat = "{0}-Windows-x64-v{1}.7z"           # Param 0: PackageTitle, Param 1: Version
$Script:PortableArchiveFormat = "{0}-Windows-x64-Portable-v{1}.7z"  # Param 0: PackageTitle, Param 1: Version
$Script:LogFileFormat = "{0}.build.{1}.log"                         # Param 0: PackageTitle, Param 1: Timestamp

# Post-build customization
$Script:RemoveCreateDump = $true # Remove createdump.exe from deps.json
$Script:RemoveXmlFiles = $true # Remove *.xml doc files from output

# 7-Zip
$Script:SevenZipPath = Join-Path $PSScriptRoot "7z/7za.exe"

# Cached values
$Script:BuildVersion = $null
$Script:AppNameCache = $null
$Script:LogFile = $null
$Script:SdkVersion = "N/A"
$Script:GitBranch = ""
$Script:GitCommit = ""
$Script:GitInfoCache = $null

# Menu item definitions for layout and logging
$Script:MenuItems = [ordered]@{
    "1" = @{ Description = "Build & Run (Debug)"; Action = { Start-BuildAndRun -Configuration "Debug" }; Response = "WaitForEnter" }
    "2" = @{ Description = "Build & Run (Release)"; Action = { Start-BuildAndRun -Configuration "Release" }; Response = "WaitForEnter" }
    "3" = @{ Description = "Watch & Run (Hot Reload)"; Action = { Watch-And-Run }; Response = "WaitForEnter" }
    "4" = @{ Description = "Restore NuGet Packages"; Action = { Restore-NuGetPackages }; Response = "WaitForEnter" }
    "5" = @{ Description = "List Packages + Updates"; Action = { Get-OutdatedPackages; Invoke-Item (Get-LogFile) }; Response = "WaitForEnter" }
    "6" = @{ Description = "Run Unit Tests"; Action = { Invoke-UnitTests }; Response = "WaitForEnter" }
    "7" = @{ Description = "Produce Changelog"; Action = { New-ChangelogFromGit }; Response = "WaitForEnter" }
    "8" = @{ Description = "Publish Portable Package"; Action = { Publish-Portable }; Response = "WaitForEnter" }
    "9" = @{ Description = "Publish Production Package"; Action = { Build-ProductionPackage }; Response = "WaitForEnter" }

    "A" = @{ Description = "Change Version Number"; Action = { Update-VersionNumber }; Response = "WaitForEnter" }
    "B" = @{ Description = "Open Solution in IDE"; Action = { Open-SolutionInIDE }; Response = "PauseBriefly" }
    "C" = @{ Description = "Clean Solution"; Action = { Remove-BuildOutput }; Response = "PauseBriefly" }
    "D" = @{ Description = "Clean Logs"; Action = { Clear-Logs }; Response = "PauseBriefly" }
    "E" = @{ Description = "Open Output Folder"; Action = { Open-OutputFolder }; Response = "PauseBriefly" }
    "F" = @{ Description = "Open User Data Folder"; Action = { Open-UserDataFolder }; Response = "PauseBriefly" }
    "G" = @{ Description = "Open Log File"; Action = { Open-LatestLogFile }; Response = "PauseBriefly" }
}

# --- Standardized Result Class ---
class CommandResult {
    [bool]$Success
    [string]$Message
    [int]$ExitCode
    [object]$Data

    CommandResult([bool]$success, [string]$message, [int]$exitCode, [object]$data) {
        $this.Success = $success
        $this.Message = $message
        $this.ExitCode = $exitCode
        $this.Data = $data
    }

    static [CommandResult]Ok([string]$message, [object]$data) {
        return [CommandResult]::new($true, $message, 0, $data)
    }

    static [CommandResult]Ok([string]$message) {
        return [CommandResult]::new($true, $message, 0, $null)
    }

    static [CommandResult]Fail([string]$message, [int]$exitCode) {
        return [CommandResult]::new($false, $message, $exitCode, $null)
    }

    static [CommandResult]Fail([string]$message) {
        return [CommandResult]::new($false, $message, -1, $null)
    }
}

# --- Utility Functions ---
function Test-Prerequisites {
    if (-not (Test-Path $Script:SolutionFile)) {
        throw "Solution file not found at '$($Script:SolutionFile)'. Ensure this script is in the correct project directory."
    }
    
    $dotNetCheck = Test-DotNetVersion
    if (-not $dotNetCheck.Success) {
        throw $dotNetCheck.Message
    }

    if ($Script:UseVelopack) {
        if (-not (Get-Command vpk -ErrorAction SilentlyContinue)) {
            Write-Log "Velopack tool 'vpk' not found. Attempting to install it globally..." "WARN"
            $installResult = Invoke-DotnetCommand -Command "tool" -Arguments "install -g vpk"
            if (-not $installResult.Success) {
                throw "Failed to install Velopack tool 'vpk'. Please install it manually with: dotnet tool install -g vpk"
            }
            Write-Log "Velopack tool 'vpk' installed successfully." "SUCCESS"
        }
    }
}

function Get-EffectiveAppName {
    if ($null -ne $Script:AppNameCache) {
        return $Script:AppNameCache
    }

    $appName = $Script:MainProjectName # Default value

    if (Test-Path $Script:MainProjectFile) {
        try {
            $csprojContent = [xml](Get-Content $Script:MainProjectFile -Raw)
            $assemblyNameNode = $csprojContent.SelectSingleNode("//PropertyGroup/AssemblyName")

            if ($null -ne $assemblyNameNode -and -not [string]::IsNullOrWhiteSpace($assemblyNameNode.InnerText)) {
                $appName = $assemblyNameNode.InnerText.Trim()
                Write-Log "Found AssemblyName in project file: $appName" "DEBUG"
            }
            else {
                Write-Log "AssemblyName not found in project file, defaulting to MainProjectName: $appName" "DEBUG"
            }
        }
        catch {
            Write-Log "Error parsing AssemblyName from project file: $_. Using default: $appName" "WARN"
        }
    }

    $Script:AppNameCache = $appName
    return $appName
}

function Get-CachedBuildVersion {
    if ($null -eq $Script:BuildVersion) {
        $versionResult = Get-BuildVersion
        if ($versionResult.Success) {
            $Script:BuildVersion = $versionResult.Data
            Write-Log "Build version cached: $Script:BuildVersion" "DEBUG"
        }
        else {
            Write-Log "Failed to get build version: $($versionResult.Message)" "ERROR"
            return $null
        }
    }
    return $Script:BuildVersion
}

function Clear-BuildVersionCache {
    $Script:BuildVersion = $null
    Write-Log "Build version cache cleared" "DEBUG"
}

function Get-GitInfo {
    if ($null -ne $Script:GitInfoCache) {
        return $Script:GitInfoCache
    }

    $gitInfo = @{ Branch = "N/A"; Commit = "N/A" }

    if (Get-Command git -ErrorAction SilentlyContinue) {
        try {
            $gitInfo.Branch = (git -C $Script:RepoRoot rev-parse --abbrev-ref HEAD 2>$null).Trim()
            $gitInfo.Commit = (git -C $Script:RepoRoot rev-parse --short HEAD 2>$null).Trim()

            if ([string]::IsNullOrWhiteSpace($gitInfo.Branch)) { $gitInfo.Branch = "N/A" }
            if ([string]::IsNullOrWhiteSpace($gitInfo.Commit)) { $gitInfo.Commit = "N/A" }
        }
        catch {
            Write-Log "Failed to get Git info: $_" "WARN"
        }
    }
    else {
        Write-Log "Git not found in PATH" "WARN"
    }

    $Script:GitInfoCache = $gitInfo
    return $gitInfo
}

function Test-PathExists {
    param([string]$Path, [string]$Description)

    if (-not (Test-Path $Path)) {
        Write-Log "$Description not found: $Path" "ERROR"
        return $false
    }
    return $true
}

function Remove-FilesByPattern {
    param([string]$Path, [string[]]$Patterns)

    $files = Get-ChildItem -Path $Path -Include $Patterns -Recurse -File -ErrorAction SilentlyContinue
    if ($files) {
        $files | Remove-Item -Force -ErrorAction SilentlyContinue
        return $files.Count
    }
    return 0
}

function Invoke-WithStandardErrorHandling {
    param(
        [scriptblock]$Action,
        [string]$SuccessMessage,
        [string]$FailureMessage,
        [bool]$LogError = $true
    )

    $result = & $Action
    if ($result.Success) {
        if ($SuccessMessage) { Write-Log $SuccessMessage "SUCCESS" }
        return $result.Data
    }
    else {
        if ($LogError) { Write-Log "$FailureMessage : $($result.Message)" "ERROR" }
        return $null
    }
}

function Clear-ScreenRobust {
    # Force clear all progress bars by completing them
    try {
        Write-Progress -Completed
        Write-Progress -Activity " " -Status " " -Completed
        # Add a small delay to ensure progress bars are cleared
        Start-Sleep -Milliseconds 50
    }
    catch { }

    # Clear the screen using the most reliable method
    try {
        # This is the most reliable way to clear the screen in PowerShell
        $Host.UI.RawUI.Clear()
    }
    catch {
        try {
            Clear-Host
        }
        catch {
            try {
                [System.Console]::Clear()
            }
            catch {
                # Nuclear option - write enough blank lines to push everything out
                Write-Host ("`n" * 50)
            }
        }
    }

    # Ensure output is flushed
    [System.Console]::Out.Flush()
    [System.Console]::Error.Flush()
}

# --- Logging ---
function Get-LogFile {
    if ($null -eq $Script:LogFile) {
        $Script:LogFile = Start-Logging
    }
    return $Script:LogFile
}

function Start-Logging {
    $logDir = Join-Path $PSScriptRoot "Logs"
    if (-not (Test-Path $logDir)) {
        New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    }

    $timestamp = (Get-Date).ToString('yyyyMMdd.HHmmss')
    $logFileName = [string]::Format($Script:LogFileFormat, $Script:PackageTitle, $timestamp)
    $logFile = Join-Path $logDir $logFileName
    $appVersion = "N/A" # Defer version lookup to avoid recursion

    $header = @"
***************************************
$($Script:PackageTitle), version = $($appVersion)
Log Start: $((Get-Date).ToString('yyyyMMddHHmmss'))
Username:   $($env:USERDOMAIN)\$($env:USERNAME)
***************************************
"@

    Set-Content -Path $logFile -Value $header
    return $logFile
}

function Write-Log {
    param([string]$Message, [string]$Level = "INFO")

    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logEntry = "[$timestamp] [$Level] $Message"

    Add-Content -Path (Get-LogFile) -Value $logEntry

    # Only write specific levels to the console. INFO is now log-only.
    switch ($Level) {
        "ERROR" { Write-Host $Message -ForegroundColor Red }
        "WARN" { Write-Host $Message -ForegroundColor Yellow }
        "SUCCESS" { Write-Host $Message -ForegroundColor Green }
        "CONSOLE" { Write-Host $Message }
    }
}

function Clear-Logs {
    Write-Log "Cleaning logs..." "CONSOLE"
    $logDir = Join-Path $PSScriptRoot "Logs"

    if (Test-Path $logDir) {
        $logFiles = Get-ChildItem -Path $logDir -Filter "*.log" -File
        $currentLog = Get-LogFile

        $oldLogs = $logFiles | Where-Object { $_.FullName -ne $currentLog }

        if ($oldLogs.Count -gt 0) {
            Write-Log "Removing $($oldLogs.Count) old log files from $logDir"
            $oldLogs | Remove-Item -Force -ErrorAction SilentlyContinue
            Write-Log "Log cleanup successful." "SUCCESS"
        }
        else {
            Write-Log "No old log files to clean."
        }
    }
    else {
        Write-Log "Log directory not found." "WARN"
    }
}

# --- Prerequisite Check ---
function Test-DotNetVersion {
    if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
        $msg = "'dotnet.exe' not found. Ensure .NET $($Script:RequiredDotNetVersion) SDK is installed and in your PATH."
        return [CommandResult]::Fail($msg)
    }

    $versionOutput = (dotnet --version 2>$null).Trim()
    if ($versionOutput -match "^$($Script:RequiredDotNetVersion)\.") {
        $Script:SdkVersion = $versionOutput
        return [CommandResult]::Ok("Found version $versionOutput", $versionOutput)
    }
    else {
        $msg = "Required version $($Script:RequiredDotNetVersion).*, but found $versionOutput. Please install the correct SDK."
        return [CommandResult]::Fail($msg)
    }
}

# --- Command Execution Helper ---
function Invoke-DotnetCommand {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Command,
        [Parameter(Mandatory = $true)]
        [string]$Arguments,
        [string]$Verbosity = "normal",
        [switch]$IgnoreErrors
    )

    $fullArgs = "$Command $Arguments --verbosity $Verbosity"
    $result = Invoke-ExternalCommand -ExecutablePath "dotnet" -Arguments $fullArgs -IgnoreErrors:$IgnoreErrors

    if ($result.Success -or $IgnoreErrors) {
        return [CommandResult]::Ok("Dotnet command successful", $result)
    }
    else {
        return [CommandResult]::Fail("Dotnet command failed: $($result.Message)", $result.ExitCode)
    }
}

function Invoke-ExternalCommand {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ExecutablePath,
        [Parameter(Mandatory = $true)]
        [string]$Arguments,
        [string]$WorkingDirectory = "",
        [switch]$IgnoreErrors
    )
    
    $logFile = Get-LogFile
    Write-Log "Executing: $ExecutablePath $Arguments"
    
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

    $process = $null

    try {
        $process = [System.Diagnostics.Process]::Start($processInfo)
        
        # Synchronously read all output after the process completes. This is more reliable for build scripts.
        $output = $process.StandardOutput.ReadToEnd()
        $errors = $process.StandardError.ReadToEnd()
        
        $process.WaitForExit()

        # Log the full output to the log file for detailed analysis.
        if (-not [string]::IsNullOrWhiteSpace($output)) {
            Add-Content -Path $logFile -Value $output
        }
        if (-not [string]::IsNullOrWhiteSpace($errors)) {
            Add-Content -Path $logFile -Value "ERROR: $errors"
        }

        if ($process.ExitCode -eq 0 -or $IgnoreErrors) {
            return [CommandResult]::Ok("External command successful", @{ ExitCode = $process.ExitCode })
        }
        else {
            # On failure, combine all output to give the user maximum context.
            $fullOutput = "Process exited with code $($process.ExitCode)."
            if (-not [string]::IsNullOrWhiteSpace($output)) {
                $fullOutput += "`n--- STDOUT ---`n$($output.Trim())"
            }
            if (-not [string]::IsNullOrWhiteSpace($errors)) {
                $fullOutput += "`n--- STDERR ---`n$($errors.Trim())"
            }
            return [CommandResult]::Fail($fullOutput, $process.ExitCode)
        }
    }
    catch {
        return [CommandResult]::Fail("Error starting process: $($_.Exception.Message)")
    }
    finally {
        if ($process) { 
            if (!$process.HasExited) { 
                $process.Kill() 
            }
            $process.Dispose() 
        }
    }
}

# --- Helpers ---
function Confirm-IdeShutdown {
    [CmdletBinding()]
    param([string]$Action)

    $ideProcesses = @{
        "devenv"  = "Visual Studio";
        "Code"    = "Visual Studio Code";
        "rider64" = "JetBrains Rider"
    }

    $runningIdes = [System.Collections.Generic.List[string]]::new()
    foreach ($procName in $ideProcesses.Keys) {
        if (Get-Process -Name $procName -ErrorAction SilentlyContinue) {
            $runningIdes.Add($ideProcesses[$procName])
        }
    }

    if ($runningIdes.Count -eq 0) {
        return $true
    }

    $ideList = $runningIdes -join ', '
    $prompt = "The following IDE(s) are running: $ideList. Continuing may cause file lock errors. Continue anyway? (y/n)"
    Write-Log "IDE(s) running: $ideList. This may cause the '$Action' operation to fail." "WARN"

    $choice = Read-Host -Prompt $prompt

    if ($choice.ToLower() -eq 'y') {
        Write-Log "User chose to proceed despite running IDEs."
        return $true
    }

    Write-Log "$Action aborted by user due to running IDEs." "ERROR"
    return $false
}

function Get-BuildVersion {
    if (-not (Test-Path $Script:MainProjectFile)) {
        return [CommandResult]::Fail("Main project file not found at '$Script:MainProjectFile'")
    }

    try {
        $csprojContent = [xml](Get-Content $Script:MainProjectFile -Raw)
        $versionNode = $csprojContent.SelectSingleNode("//PropertyGroup/Version")

        if ($null -ne $versionNode) {
            return [CommandResult]::Ok("Version retrieved", $versionNode.InnerText.Trim())
        }

        return [CommandResult]::Fail("Version node not found in project file")
    }
    catch {
        return [CommandResult]::Fail("Error parsing version from project file: $_")
    }
}

function Stop-ProcessForcefully {
    param(
        [string]$ProcessName,
        [int]$TimeoutSeconds = 10
    )

    $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
    if ($processes.Count -eq 0) { return $true }

    Write-Log "Forcefully terminating $ProcessName..."
    $processes | Stop-Process -Force -ErrorAction SilentlyContinue
    return Wait-ProcessTermination -ProcessName $ProcessName -TimeoutSeconds $TimeoutSeconds
}

function Stop-ProcessGracefully {
    param(
        [string]$ProcessName,
        [int]$GracefulTimeoutSeconds = 3,
        [int]$ForcefulTimeoutSeconds = 7
    )

    $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
    if ($processes.Count -eq 0) {
        return $true
    }

    Write-Log "Attempting graceful termination of $ProcessName..."

    # Try graceful close first
    foreach ($process in $processes) {
        try {
            $process.CloseMainWindow() | Out-Null
        }
        catch {
            Write-Log "Could not send close signal to PID $($process.Id): $_" "DEBUG"
        }
    }

    $gracefulStart = Get-Date
    while (((Get-Date) - $gracefulStart).TotalSeconds -lt $GracefulTimeoutSeconds) {
        $remaining = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
        if ($remaining.Count -eq 0) {
            Write-Log "$ProcessName terminated gracefully" "SUCCESS"
            return $true
        }
        Start-Sleep -Milliseconds 250
    }

    # Fall back to forceful termination
    Write-Log "Graceful termination failed, using force..." "WARN"
    return Stop-ProcessForcefully -ProcessName $ProcessName -TimeoutSeconds $ForcefulTimeoutSeconds
}

function Confirm-ProcessTermination {
    param(
        [string]$Action = "Build",
        [int]$TerminationTimeoutSeconds = 10,
        [bool]$UseGracefulTermination = $true
    )

    $processes = Get-Process -Name $Script:ProcessNameForTermination -ErrorAction SilentlyContinue
    if ($processes.Count -eq 0) {
        return $true
    }

    $pids = $processes.Id -join ', '
    Write-Log "$Script:ProcessNameForTermination is running (PID(s): $pids)." "WARN"

    $kill = Read-Host "Do you want to terminate it? (y/n)"
    if ($kill.ToLower() -eq 'y') {
        Write-Log "Terminating $Script:ProcessNameForTermination (PIDs: $pids)..."

        $terminated = if ($UseGracefulTermination) {
            Stop-ProcessGracefully -ProcessName $Script:ProcessNameForTermination
        }
        else {
            Stop-ProcessForcefully -ProcessName $Script:ProcessNameForTermination -TimeoutSeconds $TerminationTimeoutSeconds
        }

        if ($terminated) {
            Write-Log "$Script:ProcessNameForTermination terminated." "SUCCESS"
            return $true
        }
        else {
            Write-Log "Failed to terminate $Script:ProcessNameForTermination after multiple attempts." "ERROR"
            return $false
        }
    }
    else {
        Write-Log "$Action aborted." "ERROR"
        return $false
    }
}

function Invoke-ItemSafely {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,
        [string]$ItemType = "Item"
    )

    if (-not (Test-Path $Path)) {
        Write-Log "Could not find $ItemType at '$Path'." "ERROR"
        Write-Host "Could not find $ItemType at '$Path'." -ForegroundColor Red
        return $false
    }
    else {
        try {
            Invoke-Item $Path
            return $true
        }
        catch {
            Write-Log "Failed to open $ItemType at '$Path': $_" "ERROR"
            Write-Host "Failed to open $ItemType at '$Path'." -ForegroundColor Red
            return $false
        }
    }
}

function New-ChangelogFromGit {
    param([string]$OutputDir = $null)
    
    Write-Log "Generating changelog from Git history..." "CONSOLE"
    
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Log "git.exe not found. Skipping changelog generation." "WARN"
        return
    }

    # Define output directory
    $changelogDir = if ([string]::IsNullOrEmpty($OutputDir)) {
        Join-Path $PSScriptRoot "Changelogs"
    }
    else {
        $OutputDir
    }
    
    if (-not (Test-Path $changelogDir)) {
        New-Item -ItemType Directory -Path $changelogDir -Force | Out-Null
        Write-Log "Created changelog directory: $changelogDir"
    }

    try {
        # Use 'git tag --sort' to find the latest version tag across all branches, which is more reliable.
        $latestTag = (git -C $Script:SolutionRoot tag --sort=-v:refname | Select-Object -First 1).Trim()
        if ([string]::IsNullOrEmpty($latestTag)) { 
            throw "No tags found." 
        }
        
        Write-Log "Found latest tag: '$latestTag'. Generating changelog from new commits."
        $commitRange = "$latestTag..HEAD"
        $header = "## Changes since $latestTag"
    }
    catch {
        Write-Log "No Git tags found. Generating changelog for all commits." "WARN"
        $commitRange = "HEAD"
        $header = "## All Changes"
    }

    try {
        # Get commits with hash, subject, and author - oldest first
        $gitLogCommand = @("log", $commitRange, "--pretty=format:%h|%s|%an", "--reverse")
        $commitData = git -C $Script:SolutionRoot $gitLogCommand 2>$null

        # Build changelog content
        $changelogContent = @()
        $changelogContent += "# $($Script:PackageTitle) Changelog"
        $changelogContent += ""
        $changelogContent += $header
        $changelogContent += ""

        if ([string]::IsNullOrWhiteSpace($commitData)) {
            Write-Log "No new commits to add to changelog." "WARN"
            $changelogContent += "- No changes since the last version."
        }
        else {
            # Process each commit and format as bullet points
            $commitLines = $commitData -split "`n"
            foreach ($line in $commitLines) {
                $parts = $line -split "\|", 3
                if ($parts.Count -eq 3) {
                    $hash = $parts[0].Trim()
                    $subject = $parts[1].Trim()
                    $author = $parts[2].Trim()
                    $changelogContent += "- $subject by $author ($hash)"
                }
            }
        }

        $fullContent = $changelogContent -join "`n"
        
        # Determine filename based on whether an output directory was specified.
        $outputPath = if ([string]::IsNullOrEmpty($OutputDir)) {
            # Create timestamped filename for local generation
            $timestamp = (Get-Date).ToString('yyyyMMdd-HHmmss')
            Join-Path $changelogDir "Changelog-$timestamp.md"
        }
        else {
            # Create standard filename for releases
            Join-Path $changelogDir "changelog.md"
        }
        
        Set-Content -Path $outputPath -Value $fullContent -Encoding UTF8
        
        Write-Log "Changelog created: $outputPath" "SUCCESS"
        
        # Only open the changelog file if it's in the default location (not release dir)
        if ([string]::IsNullOrEmpty($OutputDir)) {
            Invoke-ItemSafely -Path $outputPath -ItemType "Changelog file"
        }
    }
    catch {
        Write-Log "Failed to generate changelog from Git history: $_" "WARN"
    }
}

function Compress-With7Zip {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourceDir,
        [Parameter(Mandatory = $true)]
        [string]$ArchivePath
    )

    if (-not (Test-Path $Script:SevenZipPath)) {
        Write-Log "7-Zip not found at '$Script:SevenZipPath'. Using Compress-Archive instead." "WARN"

        $parentDir = Split-Path $ArchivePath -Parent
        if (-not (Test-Path $parentDir)) {
            New-Item -ItemType Directory -Path $parentDir -Force | Out-Null
        }

        try {
            Compress-Archive -Path "$SourceDir\*" -DestinationPath $ArchivePath -Force
            return [CommandResult]::Ok("Archive created using Compress-Archive", $ArchivePath)
        }
        catch {
            return [CommandResult]::Fail("Compress-Archive failed: $_")
        }
    }

    $sevenZipArgs = "a -t7z -mx=3 `"$ArchivePath`" `"$SourceDir\*`""
    $result = Invoke-ExternalCommand -ExecutablePath $Script:SevenZipPath -Arguments $sevenZipArgs

    if ($result.Success) {
        return [CommandResult]::Ok("7-Zip archive created at", $ArchivePath)
    }
    else {
        return [CommandResult]::Fail("7-Zip archiving failed: $($result.Message)")
    }
}

function Remove-CreateDumpReference {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )
    
    Write-Log "Removing createdump reference..."
    $depjsonPath = Join-Path $Path "$($Script:AppName).deps.json"
    
    if (Test-Path $depjsonPath) {
        try {
            $depjson = Get-Content $depjsonPath -Raw
            $pattern = '(?s)("createdump\.exe":\s*\{[^}]*\},?\s*)'
            $cleanedJson = $depjson -replace $pattern, ''
            Set-Content -Path $depjsonPath -Value $cleanedJson -Encoding UTF8
            Write-Log "Createdump reference removed from deps.json"
        }
        catch {
            Write-Log "Failed to remove createdump reference: $_" "ERROR"
        }
    }
    
    $createDumpExe = Join-Path $Path "createdump.exe"
    if (Test-Path $createDumpExe) {
        Remove-Item -Path $createDumpExe -Force -ErrorAction SilentlyContinue
        Write-Log "Createdump.exe removed"
    }
}

# --- Core Functions (Menu Actions) ---
function Get-OutdatedPackages {
    Write-Log "Ensuring packages are restored first..." "CONSOLE"

    $restoreResult = Invoke-DotnetCommand -Command "restore" -Arguments "`"$Script:SolutionFile`""
    if (-not $restoreResult.Success) {
        Write-Log "Package restore failed: $($restoreResult.Message)" "ERROR"
        return
    }

    Write-Log "Checking for outdated NuGet packages..." "CONSOLE"

    $process = $null
    try {
        $logFile = Get-LogFile
        $arguments = "list `"$($Script:SolutionFile)`" package --outdated"

        Write-Log "Executing: dotnet $arguments"

        # Use synchronous process execution to avoid race conditions with log file writing.
        $processInfo = New-Object System.Diagnostics.ProcessStartInfo
        $processInfo.FileName = "dotnet"
        $processInfo.Arguments = "$arguments --verbosity normal"
        $processInfo.RedirectStandardOutput = $true
        $processInfo.RedirectStandardError = $true
        $processInfo.UseShellExecute = $false
        $processInfo.CreateNoWindow = $true

        $process = [System.Diagnostics.Process]::Start($processInfo)
        $output = $process.StandardOutput.ReadToEnd()
        $stdError = $process.StandardError.ReadToEnd()
        $process.WaitForExit()

        if (-not [string]::IsNullOrWhiteSpace($output)) {
            Add-Content -Path $logFile -Value $output
        }
        if (-not [string]::IsNullOrWhiteSpace($stdError)) {
            Add-Content -Path $logFile -Value "ERROR: $($stdError)"
        }

        # Display a filtered summary to the user for immediate feedback
        if (-not [string]::IsNullOrWhiteSpace($output)) {
            $outputLines = $output.Split([Environment]::NewLine)
            $startIndex = -1
            for ($i = 0; $i -lt $outputLines.Length; $i++) {
                if ($outputLines[$i] -match 'has (no updates|the following updates)') {
                    $startIndex = $i
                    break
                }
            }

            if ($startIndex -ge 0) {
                $summary = $outputLines[$startIndex..($outputLines.Length - 1)] -join [Environment]::NewLine
                Write-Host $summary.Trim()
            }
            else {
                Write-Log "Could not find package update summary in the output. See log for details." "WARN"
            }
        }
        if (-not [string]::IsNullOrWhiteSpace($stdError)) {
            Write-Host $stdError -ForegroundColor Red
        }

        if ($process.ExitCode -eq 0) {
            Write-Log "Package check successful." "SUCCESS"
        }
        else {
            Write-Log "Package check failed. Exit code: $($process.ExitCode)" "ERROR"
        }
    }
    catch {
        Write-Log "Failed to execute package check: $_" "ERROR"
    }
    finally {
        if ($process) { $process.Dispose() }
    }
}

function Start-BuildAndRun {
    param([string]$Configuration)

    if (-not (Confirm-ProcessTermination)) { return }

    Write-Log "Building solution in $Configuration mode for $($Script:BuildPlatform) platform..." "CONSOLE"

    $arguments = "`"$Script:SolutionFile`" -c $Configuration -p:Platform=$($Script:BuildPlatform)"
    $buildResult = Invoke-DotnetCommand -Command "build" -Arguments $arguments
    if (-not $buildResult.Success) {
        Write-Log "Build failed: $($buildResult.Message)" "ERROR"
        return
    }

    Write-Log "Build successful. Starting application..." "SUCCESS"

    $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent
    $exePath = Join-Path $mainProjectDir "bin\$($Script:BuildPlatform)\$Configuration\$($Script:TargetFramework)\$($Script:PublishRuntimeId)\$($Script:AppName).exe"

    if (Test-Path $exePath) {
        Start-Process -FilePath $exePath
        Write-Log "Application started." "SUCCESS"
    }
    else {
        Write-Log "Main application EXE not found at $exePath" "ERROR"
    }
}

function Watch-And-Run {
    if (-not (Confirm-ProcessTermination)) { return }

    Write-Log "Starting dotnet watch. Press CTRL+C in the new window to stop." "CONSOLE"
    $arguments = @("watch", "--project", "`"$($Script:MainProjectFile)`"", "run")

    Start-Process -FilePath "dotnet" -ArgumentList $arguments
}

function Restore-NuGetPackages {
    if (-not (Confirm-IdeShutdown -Action "Restore NuGet Packages")) { return }

    Write-Log "Restoring NuGet packages..." "CONSOLE"

    $result = Invoke-DotnetCommand -Command "restore" -Arguments "`"$Script:SolutionFile`""

    Clear-BuildVersionCache

    if ($result.Success) {
        Write-Log "NuGet packages restored." "SUCCESS"
    }
    else {
        Write-Log "NuGet packages restore failed: $($result.Message)" "ERROR"
    }
}

function Publish-Portable {
    if (-not (Confirm-IdeShutdown -Action "Publish Portable Package")) { return }
    if (-not (Confirm-ProcessTermination -Action "Publish")) { return }

    $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent
    $baseOutputDir = Join-Path $mainProjectDir "bin\Release\$($Script:TargetFramework)\$($Script:PublishRuntimeId)"
    $publishDir = Join-Path $baseOutputDir "publish"
    $packageDir = Join-Path $baseOutputDir "packages"

    Remove-BuildOutput -NoConfirm

    if (Test-Path $publishDir) {
        Remove-Item $publishDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    $arguments = "`"$Script:MainProjectFile`" -c Release -r $Script:PublishRuntimeId --self-contained true"
    $result = Invoke-DotnetCommand -Command "publish" -Arguments $arguments

    if (-not $result.Success) {
        Write-Log $result.Message "ERROR"
        return
    }

    Write-Log "Post-build processing..." "CONSOLE"

    if ($Script:RemoveCreateDump) {
        Remove-CreateDumpReference -Path $publishDir
    }

    if ($Script:RemoveXmlFiles) {
        $removedCount = Remove-FilesByPattern -Path $publishDir -Patterns @("*.xml")
        Write-Log "Removed $removedCount documentation files (*.xml)."
    }

    Write-Log "Adding portable mode marker..."
    $portableMarkerPath = Join-Path $publishDir $Script:PortableMarkerFile
    Set-Content -Path $portableMarkerPath -Value "This file enables portable mode. Do not delete."

    $removedPdbCount = Remove-FilesByPattern -Path $publishDir -Patterns @("*.pdb")
    Write-Log "Removed $removedPdbCount debug symbols (*.pdb)."

    Write-Log "Archiving portable package..." "CONSOLE"

    if (-not (Test-Path $packageDir)) {
        New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    }

    $version = Get-CachedBuildVersion
    if ([string]::IsNullOrEmpty($version)) { $version = "1.0.0" }
    $archiveFileName = [string]::Format($Script:PortableArchiveFormat, $Script:PackageTitle, $version)
    $destinationArchive = Join-Path $packageDir $archiveFileName

    if (Test-Path $destinationArchive) {
        Remove-Item $destinationArchive -Force
    }

    $archiveResult = Compress-With7Zip -SourceDir $publishDir -ArchivePath $destinationArchive
    if (-not $archiveResult.Success) {
        Write-Log "7-Zip archiving failed: $($archiveResult.Message)" "ERROR"
        return
    }

    New-ChangelogFromGit -OutputDir $packageDir

    Write-Log "Portable archive created: $destinationArchive" "SUCCESS"
    Invoke-ItemSafely -Path $packageDir -ItemType "Output directory"
}

function Build-ProductionPackage {
    if (-not (Confirm-IdeShutdown -Action "Production Build")) { return }
    if (-not (Confirm-ProcessTermination -Action "Production Build")) { return }

    if ($Script:UseVelopack) {
        # --- Velopack Build Path ---
        $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent
        $baseOutputDir = Join-Path $mainProjectDir "bin\Release\$($Script:TargetFramework)\$($Script:PublishRuntimeId)"
        $publishDir = Join-Path $baseOutputDir "publish"
        $releaseDir = Join-Path $Script:SolutionRoot "Releases"

        # Clean previous output
        if (Test-Path $publishDir) { Remove-Item -Recurse -Force $publishDir -ErrorAction SilentlyContinue }
        if (Test-Path $releaseDir) { Remove-Item -Recurse -Force $releaseDir -ErrorAction SilentlyContinue }
        New-Item -ItemType Directory -Path $publishDir -Force | Out-Null
        
        # Publish the application (Velopack works best with self-contained apps)
        Write-Log "Publishing self-contained application for Velopack..." "CONSOLE"
        $publishArgs = "`"$Script:MainProjectFile`" -c Release -r $Script:PublishRuntimeId --self-contained true -o `"$publishDir`""
        $buildResult = Invoke-DotnetCommand -Command "publish" -Arguments $publishArgs
        if (-not $buildResult.Success) {
            Write-Log "Release publish failed: $($buildResult.Message)" "ERROR"
            return
        }

        $version = Get-CachedBuildVersion
        if ([string]::IsNullOrEmpty($version)) {
            Write-Log "Could not determine package version from csproj." "ERROR"
            return
        }

        # Validate icon path and construct argument if it exists
        $iconArg = ""
        if (Test-Path $Script:PackageIconPath) {
            $iconArg = "--icon `"$($Script:PackageIconPath)`""
        }
        else {
            Write-Log "Icon file not found at '$($Script:PackageIconPath)'. Building without an icon." "WARN"
        }

        # The -c (--channel) argument takes a NAME, not a URL.
        $channelArg = ""
        if (-not [string]::IsNullOrEmpty($Script:VelopackChannelName)) {
            $channelArg = "-c $($Script:VelopackChannelName)"
            Write-Log "Using Velopack channel: $($Script:VelopackChannelName)"
        }

        # Package with Velopack. This single command creates the installer, portable, and update packages.
        Write-Log "Packaging with Velopack..." "CONSOLE"
        $velopackArgs = "pack --packId `"$($Script:PackageId)`" --packVersion $version --packDir `"$publishDir`" -o `"$releaseDir`" $iconArg $channelArg --verbose"
        
        # --- DIAGNOSTIC STEP ---
        # Log the exact command being run to diagnose parsing issues.
        Write-Log "DIAGNOSTIC - Executing command: vpk $velopackArgs"

        $packResult = Invoke-ExternalCommand -ExecutablePath "vpk" -Arguments $velopackArgs
        if (-not $packResult.Success) {
            Write-Log "Velopack packaging failed: $($packResult.Message)" "ERROR"
            return
        }

        # Rename the output files to the desired format.
        Write-Log "Renaming release artifacts..."
        try {
            $setupFile = Get-ChildItem -Path $releaseDir -Filter "*Setup.exe" | Select-Object -First 1
            $portableFile = Get-ChildItem -Path $releaseDir -Filter "*-portable.zip" | Select-Object -First 1
            
            if ($setupFile) {
                $newSetupName = "$($Script:PackageTitle)-Windows-x64-Setup-v$($version).exe"
                Rename-Item -Path $setupFile.FullName -NewName $newSetupName
                Write-Log "Renamed installer to $newSetupName"
            }

            if ($portableFile) {
                $newPortableName = "$($Script:PackageTitle)-Windows-x64-Portable-v$($version).zip"
                Rename-Item -Path $portableFile.FullName -NewName $newPortableName
                Write-Log "Renamed portable package to $newPortableName"
            }
        }
        catch {
            Write-Log "Could not rename output files: $_" "WARN"
        }

        New-ChangelogFromGit -OutputDir $releaseDir
        
        Write-Log "Velopack release created successfully in: $releaseDir" "SUCCESS"
        Invoke-ItemSafely -Path $releaseDir -ItemType "Release directory"
    }
    else {
        # --- 7-Zip Build Path ---
        $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent
        $baseOutputDir = Join-Path $mainProjectDir "bin\Release\$($Script:TargetFramework)\$($Script:PublishRuntimeId)"
        $releaseDir = Join-Path $baseOutputDir "release_packages"
        $stagingDir = Join-Path $baseOutputDir "production_staging"

        # Clean previous output
        if (Test-Path $releaseDir) { Remove-Item -Recurse -Force $releaseDir -ErrorAction SilentlyContinue }
        if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir -ErrorAction SilentlyContinue }
        New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
        New-Item -ItemType Directory -Path $stagingDir -Force | Out-Null

        # Publish once to staging directory
        Write-Log "Building release package..." "CONSOLE"
        $publishArgs = "`"$Script:MainProjectFile`" -c Release -o `"$stagingDir`" --self-contained true -p:PublishSingleFile=true -p:PublishReadyToRun=true"
        $buildResult = Invoke-DotnetCommand -Command "publish" -Arguments $publishArgs
        if (-not $buildResult.Success) {
            Write-Log "Release build failed: $($buildResult.Message)" "ERROR"
            return
        }

        # Post-build cleanup
        $removedPdbCount = Remove-FilesByPattern -Path $stagingDir -Patterns @("*.pdb")
        Write-Log "Removed $removedPdbCount debug symbols"

        $version = Get-CachedBuildVersion
        if ([string]::IsNullOrEmpty($version)) { $version = "1.0.0" }

        # Create standard package
        Write-Log "Archiving standard package..." "CONSOLE"
        $stdArchiveName = [string]::Format($Script:StandardArchiveFormat, $Script:PackageTitle, $version)
        $stdArchivePath = Join-Path $releaseDir $stdArchiveName
        $archiveResult = Compress-With7Zip -SourceDir $stagingDir -ArchivePath $stdArchivePath
        if (-not $archiveResult.Success) {
            Write-Log "Standard release archiving failed: $($archiveResult.Message)" "ERROR"
            return
        }

        # Create portable package
        Write-Log "Archiving portable package..." "CONSOLE"
        $portableMarkerPath = Join-Path $stagingDir $Script:PortableMarkerFile
        Set-Content -Path $portableMarkerPath -Value "This file enables portable mode. Do not delete."

        $portableArchiveName = [string]::Format($Script:PortableArchiveFormat, $Script:PackageTitle, $version)
        $portableArchivePath = Join-Path $releaseDir $portableArchiveName
        $portableArchiveResult = Compress-With7Zip -SourceDir $stagingDir -ArchivePath $portableArchivePath
        if (-not $portableArchiveResult.Success) {
            Write-Log "Portable release archiving failed: $($portableArchiveResult.Message)" "ERROR"
            return
        }

        New-ChangelogFromGit -OutputDir $releaseDir

        # Cleanup staging
        if (Test-Path $stagingDir) {
            Remove-Item -Recurse -Force $stagingDir -ErrorAction SilentlyContinue
        }

        Write-Log "Full release package created at: $releaseDir" "SUCCESS"
        Invoke-ItemSafely -Path $releaseDir -ItemType "Release directory"
    }
}

function Remove-BuildOutput {
    param([switch]$NoConfirm)
    
    if (-not $NoConfirm) {
        if (-not (Confirm-Ideshutdown -Action "Clean Solution")) { return }
        if (-not (Confirm-ProcessTermination -Action "Clean")) { return }
    }

    Write-Log "Cleaning build files..." "CONSOLE"
    
    $buildDirs = Get-ChildItem -Path $Script:SolutionRoot -Include "bin", "obj" -Recurse -Directory -ErrorAction SilentlyContinue
    if ($buildDirs) {
        $counter = 0
        $total = $buildDirs.Count
        foreach ($dir in $buildDirs) {
            $counter++
            Write-Progress -Activity "Cleaning build directories" -Status "Removing $($dir.Name)" -PercentComplete (($counter / $total) * 100)
            Remove-Item -Path $dir.FullName -Recurse -Force -ErrorAction SilentlyContinue
        }
        # Ensure progress bar is completed AND cleared from display
        Write-Progress -Activity "Cleaning build directories" -Completed
        # Small delay to let PowerShell process the completion
        Start-Sleep -Milliseconds 50
        Write-Log "Removed $($buildDirs.Count) build directories"
    }

    if ($Script:UseVelopack) {
        $releaseDir = Join-Path $Script:SolutionRoot "Releases"
        if (Test-Path $releaseDir) {
            Write-Log "Removing Velopack releases directory..."
            Remove-Item -Path $releaseDir -Recurse -Force -ErrorAction SilentlyContinue
            Write-Log "Removed Velopack releases directory."
        }
    }

    Clear-BuildVersionCache
    Write-Log "Cleanup successful." "SUCCESS"
}

function Update-VersionNumber {
    if (-not (Confirm-IdeShutdown -Action "Change Version Number")) { return }

    if (-not (Test-Path $Script:MainProjectFile)) {
        Write-Log "Project file not found at: $($Script:MainProjectFile)" "ERROR"
        return
    }

    $currentVersion = Get-CachedBuildVersion
    Write-Log "Current version: $currentVersion" "CONSOLE"

    $newVersion = Read-Host "Enter new version, or 'X' to cancel"
    if ([string]::IsNullOrWhiteSpace($newVersion) -or $newVersion.ToLower() -eq 'x') {
        Write-Log "Operation cancelled." "INFO"
        return
    }

    try {
        if ($newVersion -notmatch '^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?$') {
            throw "Invalid version format. Use Semantic Versioning (e.g., 1.2.3 or 1.2.3-beta1)."
        }

        $csprojContent = Get-Content $Script:MainProjectFile -Raw
        $csproj = [xml]$csprojContent
        $propertyGroup = $csproj.SelectSingleNode("//PropertyGroup[Version]")

        if ($null -eq $propertyGroup) {
            $propertyGroup = $csproj.SelectSingleNode("//PropertyGroup[1]")
        }

        if ($null -ne $propertyGroup.Version) {
            $propertyGroup.Version = $newVersion
        }
        else {
            $versionElement = $csproj.CreateElement("Version")
            $versionElement.InnerText = $newVersion
            $propertyGroup.AppendChild($versionElement) | Out-Null
        }

        # Detect existing indentation to preserve file formatting.
        $indentChars = "    " # Default to 4 spaces
        $match = $csprojContent | Select-String -Pattern '(?m)^(\s+)<PropertyGroup>'
        if ($null -ne $match) {
            $indentChars = $match.Matches[0].Groups[1].Value
        }
        else {
            Write-Log "Could not detect project file indentation. Defaulting to 4 spaces." "WARN"
        }

        # Use a custom XmlWriter to preserve indentation.
        $writerSettings = New-Object System.Xml.XmlWriterSettings
        $writerSettings.Indent = $true
        $writerSettings.IndentChars = $indentChars
        $writerSettings.Encoding = [System.Text.UTF8Encoding]::new($false) # UTF-8 without BOM

        $xmlWriter = $null
        try {
            $xmlWriter = [System.Xml.XmlWriter]::Create($Script:MainProjectFile, $writerSettings)
            $csproj.Save($xmlWriter)
        }
        finally {
            if ($null -ne $xmlWriter) {
                $xmlWriter.Close()
            }
        }

        $Script:BuildVersion = $newVersion
        Write-Log "Version updated to $newVersion in $($Script:MainProjectFile)" "SUCCESS"
    }
    catch {
        Write-Log "Failed to update version: $_" "ERROR"
    }
}

function Open-LatestLogFile {
    $logDir = Join-Path $PSScriptRoot "Logs"

    if (Test-Path $logDir) {
        $latestLog = Get-ChildItem -Path $logDir -Filter "*.log" -File |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1

        if ($latestLog) {
            $success = Invoke-ItemSafely -Path $latestLog.FullName -ItemType "Log file"
            if (-not $success) {
                return
            }
        }
        else {
            Write-Log "No logs found to open." "WARN"
            Write-Host "No logs found to open." -ForegroundColor Yellow
        }
    }
    else {
        Write-Log "Log directory not found." "WARN"
        Write-Host "Log directory not found." -ForegroundColor Yellow
    }
}

function Open-UserDataFolder {
    $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent

    # The portable version is staged in the 'publish' subdirectory during a portable build.
    $publishPath = Join-Path $mainProjectDir "bin\$($Script:BuildPlatform)\Release\$($Script:TargetFramework)\$($Script:PublishRuntimeId)\publish"
    $portableMarkerPath = Join-Path $publishPath $Script:PortableMarkerFile

    $userDataPath = if (Test-Path $portableMarkerPath) {
        Join-Path $publishPath "Data"
    }
    else {
        Join-Path $env:APPDATA $Script:AppDataFolderName
    }

    $success = Invoke-ItemSafely -Path $userDataPath -ItemType "User data folder"
    if (-not $success) {
        return
    }
}

function Open-OutputFolder {
    $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent
    $outputPath = Join-Path $mainProjectDir "bin"
    $success = Invoke-ItemSafely -Path $outputPath -ItemType "Output folder"
    if (-not $success) {
        return
    }
}

function Open-SolutionInIDE {
    $success = Invoke-ItemSafely -Path $Script:SolutionFile -ItemType "Solution file"
    if (-not $success) {
        return
    }
}

function Invoke-UnitTests {
    Write-Log "Running unit tests for solution..." "CONSOLE"

    $result = Invoke-DotnetCommand -Command "test" -Arguments "`"$Script:SolutionFile`""

    if ($result.Success) {
        Write-Log "Test run successful. Check log for details (e.g., if no tests were found)." "SUCCESS"
    }
    else {
        Write-Log "One or more tests failed. Check the log file for details." "ERROR"
        Invoke-Item (Get-LogFile)
    }
}

# --- Menu Display ---
function Show-Menu {
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

    Write-Host "-------------------------------------------------------------------------------" -ForegroundColor Green
    Write-Host "              .NET Builder Toolbox for $($Script:PackageTitle)" -ForegroundColor Green
    Write-Host "-------------------------------------------------------------------------------" -ForegroundColor Green
    Write-Host "Solution: $($Script:SolutionFile)" -ForegroundColor DarkGray

    if ($Script:GitBranch -ne "N/A" -and $Script:GitCommit -ne "N/A") {
        Write-Host "Branch:   $($Script:GitBranch) ($($Script:GitCommit))" -ForegroundColor DarkGray
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

function Wait-ProcessTermination {
    param(
        [string]$ProcessName,
        [int]$TimeoutSeconds = 10,
        [int]$CheckIntervalMs = 500
    )

    Write-Log "Waiting for $ProcessName to terminate (timeout: ${TimeoutSeconds}s)..."
    $startTime = Get-Date
    $attempt = 0

    while ($true) {
        $processes = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
        if ($processes.Count -eq 0) {
            Write-Log "$ProcessName terminated after $attempt attempts"
            return $true
        }

        $elapsed = (Get-Date) - $startTime
        if ($elapsed.TotalSeconds -ge $TimeoutSeconds) {
            $pids = $processes.Id -join ', '
            Write-Log "Timeout waiting for $ProcessName to terminate after $TimeoutSeconds seconds" "WARN"
            Write-Log "Processes still running (PIDs: $pids)" "WARN"
            return $false
        }

        $attempt++
        Start-Sleep -Milliseconds $CheckIntervalMs
    }
}

#----------------------------------------------------------------------
# --- Main Execution Logic ---
function Main {
    try {
        Test-Prerequisites
        
        # Determine the effective application name for use throughout the script.
        $Script:AppName = Get-EffectiveAppName
        $Script:ProcessNameForTermination = $Script:AppName
        Write-Log "Effective App Name set to: $($Script:AppName)"

        $gitInfo = Get-GitInfo
        $Script:GitBranch = $gitInfo.Branch
        $Script:GitCommit = $gitInfo.Commit

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

# Start the application
Main