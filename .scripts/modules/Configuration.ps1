# Builder Toolbox - Configuration & Paths

# -----------------------------------------------------------------------------
# 0. Global Environment Settings
# -----------------------------------------------------------------------------
# Suppress all PowerShell progress bars to prevent console ghosting artifacts
$ProgressPreference = 'SilentlyContinue'

# Suppress .NET CLI noise
$env:DOTNET_NOLOGO = 'true'
$env:DOTNET_SKIP_FIRST_TIME_EXPERIENCE = 'true'
$env:DOTNET_CLI_TELEMETRY_OPTOUT = 'true'

# -----------------------------------------------------------------------------
# 1. Path Discovery & Context
# -----------------------------------------------------------------------------
$Script:PSScriptRoot = (Split-Path -Parent $PSCommandPath)

# Calculate RepoRoot based on standard structure: repo/.scripts/modules/Config.ps1
try {
    $scriptDirItem = Get-Item $PSScriptRoot -ErrorAction Stop
    # .scripts/modules -> .scripts -> repo_root
    $Script:RepoRoot = $scriptDirItem.Parent.Parent.FullName
}
catch {
    $Script:RepoRoot = $PSScriptRoot
}

# -----------------------------------------------------------------------------
# 2. Configuration Loading
# -----------------------------------------------------------------------------

# Default values (Generic / Portable)
# Project-specific values are initialized to null to trigger auto-discovery
$config = @{
    PackageTitle          = $null
    MainProjectName       = $null
    SolutionFileName      = $null
    SolutionSubFolder     = ""
    MainProjectSourcePath = $null
    PackageAuthors        = "Developer"
    RequiredDotNetVersion = "8.0"        # Safe LTS default
    TargetFramework       = "net8.0-windows"
    BuildPlatform         = "x64"
    PublishRuntimeId      = "win-x64"
    UseVelopack           = $false       # Default to false for better portability
    VelopackChannelName   = "prod"
    RemoveCreateDump      = $true
    RemoveXmlFiles        = $true
    XmlKeepPatterns       = @()
    IdeProcessNames       = @{
        "devenv"  = "Visual Studio"
        "Code"    = "Visual Studio Code"
        "rider64" = "JetBrains Rider"
    }
}

# Define search paths for BuilderToolbox-config.json
$searchPaths = @(
    (Join-Path $Script:RepoRoot "BuilderToolbox-config.json"),                            # Repo Root
    (Join-Path (Split-Path $PSScriptRoot -Parent) "BuilderToolbox-config.json"),          # .scripts folder
    (Join-Path $PSScriptRoot "BuilderToolbox-config.json")                                # modules folder
)

# Diagnostic Output
# (Excessive logging silenced for cleaner startup)
# Write-Host "----------------------------------------------------------------" -ForegroundColor DarkGray
# Write-Host "Configuration Diagnostics" -ForegroundColor Cyan
# Write-Host "Repo Root:     $($Script:RepoRoot)" -ForegroundColor Gray
# Write-Host "Searching for: BuilderToolbox-config.json" -ForegroundColor Gray

$configFile = $null
foreach ($path in $searchPaths) {
    if (Test-Path $path) {
        $configFile = $path
        break
    }
}

if ($configFile) {
    # Write-Host "FOUND:         $configFile" -ForegroundColor Green
    try {
        $jsonSettings = Get-Content $configFile -Raw | ConvertFrom-Json

        # Refactor 5: Optimized JSON Mapping
        $jsonSettings.PSObject.Properties | ForEach-Object {
            if ($config.ContainsKey($_.Name)) {
                $config[$_.Name] = $_.Value
            }
        }
        # Write-Host "Status:        Settings loaded successfully." -ForegroundColor Gray
    }
    catch {
        Write-Host "ERROR:         Failed to parse JSON: $_" -ForegroundColor Red
    }
}
else {
    # Only show output if missing, as this is a potential issue
    Write-Host "MISSING:       BuilderToolbox-config.json not found. Attempting auto-discovery..." -ForegroundColor Yellow
}

# Checks for environment variables with prefix 'BUILDER_'
$envPrefix = "BUILDER_"
foreach ($key in @($config.Keys)) {
    $envVarName = "$envPrefix$($key.ToUpper())"
    if (Test-Path "env:$envVarName") {
        $envValue = (Get-Item "env:$envVarName").Value

        if ($config[$key] -is [bool]) {
            if ($envValue -match '^(1|true|yes|on)$') {
                $config[$key] = $true
            }
            elseif ($envValue -match '^(0|false|no|off)$') {
                $config[$key] = $false
            }
            else {
                try {
                    $config[$key] = [System.Convert]::ToBoolean($envValue)
                }
                catch {
                    Write-Host "WARNING:    Invalid boolean value '$envValue' for $envVarName. Ignoring override." -ForegroundColor Yellow
                    continue
                }
            }
        }
        else {
            $config[$key] = $envValue
        }

        # Silenced override confirmation
        # Write-Host "OVERRIDE:      $key set from environment variable $envVarName" -ForegroundColor Cyan
    }
}

# -----------------------------------------------------------------------------
# 3. Auto-Discovery (Portable Mode)
# -----------------------------------------------------------------------------

# A. Find Solution File
if ([string]::IsNullOrEmpty($config.SolutionFileName)) {
    $slnFiles = Get-ChildItem -Path $Script:RepoRoot -Filter "*.sln" -File
    if ($slnFiles.Count -gt 0) {
        $config.SolutionFileName = $slnFiles[0].Name
        # Write-Host "AUTO:          Discovered solution '$($config.SolutionFileName)'" -ForegroundColor Cyan
        if ($slnFiles.Count -gt 1) {
            Write-Host "WARNING:       Multiple solutions found. Using the first one." -ForegroundColor Yellow
        }
    }
}

# B. Derive Package Title
if ([string]::IsNullOrEmpty($config.PackageTitle) -and -not [string]::IsNullOrEmpty($config.SolutionFileName)) {
    $config.PackageTitle = [System.IO.Path]::GetFileNameWithoutExtension($config.SolutionFileName)
    # Write-Host "AUTO:          Derived PackageTitle '$($config.PackageTitle)'" -ForegroundColor Cyan
}

# C. Derive Main Project Name
if ([string]::IsNullOrEmpty($config.MainProjectName) -and -not [string]::IsNullOrEmpty($config.PackageTitle)) {
    # Assumption: Main project often shares the name with the Package/Solution
    $config.MainProjectName = $config.PackageTitle
}

# D. Find Main Project Path
if ([string]::IsNullOrEmpty($config.MainProjectSourcePath) -and -not [string]::IsNullOrEmpty($config.MainProjectName)) {
    $csprojName = "$($config.MainProjectName).csproj"
    $csproj = Get-ChildItem -Path $Script:RepoRoot -Filter $csprojName -Recurse -File | Select-Object -First 1

    if ($csproj) {
        # Calculate relative path from RepoRoot
        $fullDir = $csproj.Directory.FullName
        if ($fullDir.StartsWith($Script:RepoRoot)) {
            $relPath = $fullDir.Substring($Script:RepoRoot.Length).TrimStart('\', '/')
            $config.MainProjectSourcePath = $relPath
            # Write-Host "AUTO:          Discovered project path '$relPath'" -ForegroundColor Cyan
        }
    }
    else {
        # Fallback: Assume it's in the root or matches name if not found (will likely fail validation later if wrong)
        $config.MainProjectSourcePath = $config.MainProjectName
    }
}

# Clean termination of config section
# Write-Host "----------------------------------------------------------------" -ForegroundColor DarkGray

# Apply settings to Script Scope
$Script:PackageTitle = $config.PackageTitle
$Script:MainProjectName = $config.MainProjectName
$Script:SolutionFileName = $config.SolutionFileName
$Script:SolutionSubFolder = $config.SolutionSubFolder
$Script:MainProjectSourcePath = $config.MainProjectSourcePath
$Script:PackageAuthors = $config.PackageAuthors
$Script:RequiredDotNetVersion = $config.RequiredDotNetVersion
$Script:TargetFramework = $config.TargetFramework
$Script:BuildPlatform = $config.BuildPlatform
$Script:PublishRuntimeId = $config.PublishRuntimeId
$Script:UseVelopack = [bool]$config.UseVelopack
$Script:VelopackChannelName = $config.VelopackChannelName
$Script:RemoveCreateDump = [bool]$config.RemoveCreateDump
$Script:RemoveXmlFiles = [bool]$config.RemoveXmlFiles
$Script:XmlKeepPatterns = if ($config.XmlKeepPatterns) { $config.XmlKeepPatterns } else { @() }
$Script:IdeProcessNames = if ($config.IdeProcessNames) { $config.IdeProcessNames } else { @{} }

$Script:SolutionRoot = if (-not [string]::IsNullOrEmpty($Script:SolutionSubFolder)) { Join-Path $Script:RepoRoot $Script:SolutionSubFolder } else { $Script:RepoRoot }
$Script:SolutionFile = Join-Path $Script:SolutionRoot $Script:SolutionFileName

$Script:MainProjectDir = Join-Path $Script:SolutionRoot $Script:MainProjectSourcePath
$Script:MainProjectFile = Join-Path $Script:MainProjectDir "$($Script:MainProjectName).csproj"

# Velopack settings
$Script:PackageId = $Script:PackageTitle
$Script:PackageIconPath = Join-Path $Script:MainProjectDir "Assets/Icons/$($Script:PackageTitle.ToLower()).ico"
$Script:MainExeName = "$($Script:MainProjectName).exe"
$Script:AppDataFolderName = $Script:PackageTitle

# Helper to safely convert config values to boolean (handles "false" string vs false boolean)
function Convert-ToBool {
    param($Value)
    if ($Value -is [bool]) { return $Value }
    if ([string]::IsNullOrWhiteSpace($Value)) { return $false }
    try { return [System.Convert]::ToBoolean($Value) } catch { return $false }
}

$Script:UseVelopack = Convert-ToBool $config.UseVelopack
$Script:RemoveCreateDump = Convert-ToBool $config.RemoveCreateDump
$Script:RemoveXmlFiles = Convert-ToBool $config.RemoveXmlFiles

# Package naming and markers
$Script:PortableMarkerFile = "$($Script:PackageTitle.ToLower()).portable"
$Script:StandardArchiveFormat = "{0}-Windows-x64-v{1}.7z"           # Param 0: PackageTitle, Param 1: Version
$Script:PortableArchiveFormat = "{0}-Windows-x64-Portable-v{1}.7z"  # Param 0: PackageTitle, Param 1: Version
$Script:LogFileFormat = "{0}.build.{1}.log"                         # Param 0: PackageTitle, Param 1: Timestamp

# 7-Zip
$Script:SevenZipPath = $null

# Cached values
$Script:BuildVersion = $null
$Script:AppNameCache = $null
$Script:LogFile = $null
$Script:LogBuffer = [System.Collections.Generic.List[string]]::new()
$Script:LogBufferFlushThreshold = 50
$Script:SdkVersion = "N/A"
$Script:AppName = $null                     # Effective App Name
$Script:ProcessNameForTermination = $null   # Process Name for termination checks

# Menu item definitions for layout and logging
$Script:MenuItems = [ordered]@{
    "1" = @{ Description = "Build & Run (Debug)"; Command = "Start-BuildAndRun"; Args = @{ Configuration = "Debug" }; Response = "WaitForEnter" }
    "2" = @{ Description = "Build & Run (Release)"; Command = "Start-BuildAndRun"; Args = @{ Configuration = "Release" }; Response = "WaitForEnter" }
    "3" = @{ Description = "Watch & Run (Hot Reload)"; Command = "Watch-And-Run"; Args = @{}; Response = "WaitForEnter" }
    "4" = @{ Description = "Restore NuGet Packages"; Command = "Restore-NuGetPackages"; Args = @{}; Response = "WaitForEnter" }
    "5" = @{ Description = "List Packages + Updates"; Command = "Show-OutdatedPackages"; Args = @{}; Response = "WaitForEnter" }
    "6" = @{ Description = "Run Unit Tests"; Command = "Invoke-UnitTests"; Args = @{}; Response = "WaitForEnter" }
    "7" = @{ Description = "Produce Changelog"; Command = "New-ChangelogFromGit"; Args = @{}; Response = "WaitForEnter" }
    "8" = @{ Description = "Publish Portable Package"; Command = "Publish-Portable"; Args = @{}; Response = "WaitForEnter" }
    "9" = @{ Description = "Publish Production Package"; Command = "New-ProductionPackage"; Args = @{}; Response = "WaitForEnter" }

    "A" = @{ Description = "Change Version Number"; Command = "Update-VersionNumber"; Args = @{}; Response = "WaitForEnter" }
    "B" = @{ Description = "Open Solution in IDE"; Command = "Open-SolutionInIDE"; Args = @{}; Response = "PauseBriefly" }
    "C" = @{ Description = "Clean Solution"; Command = "Remove-BuildOutput"; Args = @{}; Response = "PauseBriefly" }
    "D" = @{ Description = "Clean Logs"; Command = "Clear-Logs"; Args = @{}; Response = "PauseBriefly" }
    "E" = @{ Description = "Open Output Folder"; Command = "Open-OutputFolder"; Args = @{}; Response = "PauseBriefly" }
    "F" = @{ Description = "Open User Data Folder"; Command = "Open-UserDataFolder"; Args = @{}; Response = "PauseBriefly" }
    "G" = @{ Description = "Open Log File"; Command = "Open-LatestLogFile"; Args = @{}; Response = "PauseBriefly" }
}

function Test-PrerequisiteTools {
    $tools = @{}

    # Test .NET SDK
    try {
        $dotnetVersion = & dotnet --version 2>$null
        if ($LASTEXITCODE -eq 0) {
            $tools['dotnet'] = @{
                Found      = $true
                Version    = $dotnetVersion
                Required   = $Script:RequiredDotNetVersion
                Compatible = $dotnetVersion.StartsWith($Script:RequiredDotNetVersion)
            }
        }
        else {
            $tools['dotnet'] = @{ Found = $false; Version = "N/A"; Required = $Script:RequiredDotNetVersion; Compatible = $false }
        }
    }
    catch {
        $tools['dotnet'] = @{ Found = $false; Version = "N/A"; Required = $Script:RequiredDotNetVersion; Compatible = $false }
    }

    # Test Git
    try {
        $gitVersion = & git --version 2>$null
        if ($LASTEXITCODE -eq 0) {
            $tools['git'] = @{ Found = $true; Version = $gitVersion; Required = "Any"; Compatible = $true }
        }
        else {
            $tools['git'] = @{ Found = $false; Version = "N/A"; Required = "Any"; Compatible = $false }
        }
    }
    catch {
        $tools['git'] = @{ Found = $false; Version = "N/A"; Required = "Any"; Compatible = $false }
    }

    # Test 7-Zip if not using Velopack
    if (-not $Script:UseVelopack) {
        $sevenZip = Find-7ZipExecutable
        $tools['7zip'] = @{
            Found      = (-not [string]::IsNullOrEmpty($sevenZip))
            Version    = if ($sevenZip) { "Found" } else { "N/A" }
            Required   = "Required for non-Velopack builds"
            Compatible = (-not [string]::IsNullOrEmpty($sevenZip))
        }
    }

    return $tools
}

function Test-ConfigurationValidity {
    $requiredSettings = @{
        'PackageTitle'          = [string]
        'MainProjectName'       = [string]
        'SolutionFileName'      = [string]
        'TargetFramework'       = [string]
        'PublishRuntimeId'      = [string]
        'RequiredDotNetVersion' = [string]
    }

    $errors = @()
    $warnings = @()

    # Validate required settings
    foreach ($setting in $requiredSettings.Keys) {
        $value = (Get-Variable -Name $setting -Scope Script -ErrorAction SilentlyContinue).Value

        if ([string]::IsNullOrWhiteSpace($value)) {
            $errors += "Required configuration '$setting' is not set or is empty."
        }
    }

    # Validate file paths with comprehensive checks
    $solutionPath = $Script:SolutionFile
    if (Test-Path $solutionPath) {
        $solutionInfo = Get-Item $solutionPath
        if ($solutionInfo.Extension -ne ".sln") {
            $warnings += "Solution file '$solutionPath' does not have .sln extension."
        }
        Write-Log "Solution file found: $($Script:SolutionFile)" "DEBUG"
    }
    else {
        $errors += "Solution file path resolves to non-existent location: $solutionPath"
    }

    # Validate main project path
    $mainProjectPath = $Script:MainProjectFile
    if (Test-Path $mainProjectPath) {
        $projectInfo = Get-Item $mainProjectPath
        if ($projectInfo.Extension -ne ".csproj") {
            $warnings += "Main project file '$mainProjectPath' does not have .csproj extension."
        }
        Write-Log "Main project file found: $($Script:MainProjectFile)" "DEBUG"
    }
    else {
        $errors += "Main project file path resolves to non-existent location: $mainProjectPath"
    }

    # Validate solution root directory
    if (-not (Test-Path $Script:SolutionRoot)) {
        $errors += "Solution root directory does not exist: $($Script:SolutionRoot)"
    }

    # Validate main project directory
    if (-not (Test-Path $Script:MainProjectDir)) {
        $errors += "Main project directory does not exist: $($Script:MainProjectDir)"
    }

    # Validate target framework format
    if ($Script:TargetFramework -notmatch '^net\d+\.\d+(-[\w-]+)?$') {
        $warnings += "Target framework '$($Script:TargetFramework)' may have invalid format."
    }

    # Validate publish runtime identifier
    # Regex allows: win-x64, win-x64-desktop, linux-arm64, etc.
    if ($Script:PublishRuntimeId -notmatch '^[\w-]+-[\w-]+(-[\w-]+)?$') {
        $warnings += "Publish runtime identifier '$($Script:PublishRuntimeId)' may have invalid format."
    }

    # Test prerequisite tools
    $tools = Test-PrerequisiteTools
    foreach ($tool in $tools.GetEnumerator()) {
        $toolInfo = $tool.Value
        if (-not $toolInfo.Found) {
            $errors += "Required tool '$($tool.Key)' not found. $($toolInfo.Required)"
        }
        elseif (-not $toolInfo.Compatible) {
            $errors += "Tool '$($tool.Key)' version $($toolInfo.Version) is not compatible. Required: $($toolInfo.Required)"
        }
        else {
            Write-Log "Tool '$($tool.Key)' found: $($toolInfo.Version)" "DEBUG"
        }
    }

    # Report validation results
    if ($errors.Count -gt 0) {
        Write-Host "Configuration validation failed with $($errors.Count) error(s):" -ForegroundColor Red
        $errors | ForEach-Object { Write-Host "  ✗ $_" -ForegroundColor Red }

        if ($warnings.Count -gt 0) {
            Write-Host "`nAdditional warnings:" -ForegroundColor Yellow
            $warnings | ForEach-Object { Write-Host "  ⚠ $_" -ForegroundColor Yellow }
        }

        Write-Host "`nPlease fix these issues before proceeding." -ForegroundColor Red
        return $false
    }

    if ($warnings.Count -gt 0) {
        Write-Host "Configuration validation passed with $($warnings.Count) warning(s):" -ForegroundColor Yellow
        $warnings | ForEach-Object { Write-Host "  ⚠ $_" -ForegroundColor Yellow }
    }
    else {
        Write-Log "Configuration validation passed successfully" "DEBUG"
    }

    return $true
}