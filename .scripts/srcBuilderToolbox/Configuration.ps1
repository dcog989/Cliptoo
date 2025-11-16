# Builder Toolbox - Configuration & Paths

# -----------------------------------------------------------------------------
# * Edit these settings...
# -----------------------------------------------------------------------------
$Script:PackageTitle = "Cliptoo"                    # user-facing name of the application.
$Script:MainProjectName = "Cliptoo.UI"              # name of the main .csproj file (without the extension).
$Script:SolutionFileName = "Cliptoo.sln"            # name of the .sln file.
$Script:SolutionSubFolder = ""                      # folder containing the .sln file, relative to repo root. Set to "" if .sln is at the root.
$Script:MainProjectSourcePath = "Cliptoo.UI"        # path from the solution folder to the main project's folder.
$Script:PackageAuthors = "dcog989"                  # author's name.
$Script:RequiredDotNetVersion = "10"                # major version number.
$Script:TargetFramework = "net10.0-windows"         # from project `.csproj` file.
$Script:BuildPlatform = "x64"                       # target build platform (e.g., x64, AnyCPU).
$Script:PublishRuntimeId = "win-x64"                # target runtime for publishing.
$Script:UseVelopack = $true                         # use Velopack for production builds, $false to use 7-Zip.
$Script:VelopackChannelName = "prod"                # release channel (e.g., 'prod', 'staging').


# -----------------------------------------------------------------------------
# ! Derived Settings - Do not edit below this line.
# -----------------------------------------------------------------------------

# Core project settings
$Script:PSScriptRoot = (Split-Path -Parent $PSCommandPath)
$Script:RepoRoot = (Get-Item $PSScriptRoot).Parent.Parent.FullName
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
$Script:SevenZipPath = $null

# Cached values
$Script:BuildVersion = $null
$Script:AppNameCache = $null
$Script:LogFile = $null
$Script:LogBuffer = [System.Collections.Generic.List[string]]::new()
$Script:LogBufferFlushThreshold = 10
$Script:SdkVersion = "N/A"
$Script:AppName = $null                     # Effective App Name
$Script:ProcessNameForTermination = $null   # Process Name for termination checks

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
    "9" = @{ Description = "Publish Production Package"; Action = { New-ProductionPackage }; Response = "WaitForEnter" }

    "A" = @{ Description = "Change Version Number"; Action = { Update-VersionNumber }; Response = "WaitForEnter" }
    "B" = @{ Description = "Open Solution in IDE"; Action = { Open-SolutionInIDE }; Response = "PauseBriefly" }
    "C" = @{ Description = "Clean Solution"; Action = { Remove-BuildOutput }; Response = "PauseBriefly" }
    "D" = @{ Description = "Clean Logs"; Action = { Clear-Logs }; Response = "PauseBriefly" }
    "E" = @{ Description = "Open Output Folder"; Action = { Open-OutputFolder }; Response = "PauseBriefly" }
    "F" = @{ Description = "Open User Data Folder"; Action = { Open-UserDataFolder }; Response = "PauseBriefly" }
    "G" = @{ Description = "Open Log File"; Action = { Open-LatestLogFile }; Response = "PauseBriefly" }
}