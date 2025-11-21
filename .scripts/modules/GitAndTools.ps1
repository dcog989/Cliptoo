# Builder Toolbox - Git and Tool Helpers

$Script:GitInfoCache = $null

function Find-7ZipExecutable {
    # 1. Define candidates and their dependencies
    # 7za.exe is standalone. 7z.exe requires 7z.dll.
    $candidates = @(
        @{ Name = "7za.exe"; Dependency = $null },
        @{ Name = "7z.exe"; Dependency = "7z.dll" }
    )

    # 2. Define search paths
    $searchPaths = @(
        $Script:RepoRoot,
        (Join-Path $Script:RepoRoot "7z"),
        "$env:ProgramFiles\7-Zip",
        "${env:ProgramFiles(x86)}\7-Zip"
    )

    foreach ($candidate in $candidates) {
        $exeName = $candidate.Name

        # A. Check PATH first
        $inPath = Get-Command $exeName -ErrorAction SilentlyContinue
        if ($inPath) {
            $path = $inPath.Source
            # If it has a dependency, verify it exists next to the executable
            if ($candidate.Dependency) {
                $dir = Split-Path $path -Parent
                $depPath = Join-Path $dir $candidate.Dependency
                if (Test-Path $depPath) {
                    Write-Log "Found valid $exeName (with $($candidate.Dependency)) in PATH: $path"
                    return $path
                }
            }
            else {
                Write-Log "Found $exeName in PATH: $path"
                return $path
            }
        }

        # B. Check explicit search paths
        foreach ($basePath in $searchPaths) {
            $fullPath = Join-Path $basePath $exeName
            if (Test-Path $fullPath) {
                if ($candidate.Dependency) {
                    $depPath = Join-Path $basePath $candidate.Dependency
                    if (Test-Path $depPath) {
                        Write-Log "Found valid $exeName (with $($candidate.Dependency)) in: $fullPath"
                        return $fullPath
                    }
                }
                else {
                    Write-Log "Found $exeName in: $fullPath"
                    return $fullPath
                }
            }
        }
    }

    Write-Log "7-Zip executable (7za.exe or valid 7z.exe) not found. Will fall back to PowerShell's Compress-Archive." "WARN"
    return $null
}

function Clear-GitInfoCache {
    $Script:GitInfoCache = $null
}

function Get-GitInfo {
    if ($null -ne $Script:GitInfoCache) {
        return $Script:GitInfoCache
    }

    $gitInfo = @{ Branch = "N/A"; Commit = "N/A" }

    if (Get-Command git -ErrorAction SilentlyContinue) {
        # Optimization: Check if .git directory exists before spawning git process
        # This avoids the overhead of 'git rev-parse' on non-git folders
        if (Test-Path (Join-Path $Script:SolutionRoot ".git")) {
            try {
                $branchOutput = git -C $Script:SolutionRoot rev-parse --abbrev-ref HEAD 2>$null
                if ($null -ne $branchOutput) {
                    $gitInfo.Branch = $branchOutput.Trim()
                }

                $commitOutput = git -C $Script:SolutionRoot rev-parse --short HEAD 2>$null
                if ($null -ne $commitOutput) {
                    $gitInfo.Commit = $commitOutput.Trim()
                }
            }
            catch {
                Write-Log "Failed to get Git info: $_" "WARN"
            }
        }
    }

    $Script:GitInfoCache = $gitInfo
    return $gitInfo
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
        Join-Path $Script:RepoRoot "Changelogs"
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
        $tagObj = git -C $Script:SolutionRoot tag --sort=-v:refname | Select-Object -First 1
        
        if ([string]::IsNullOrWhiteSpace($tagObj)) {
            throw "No tags found."
        }

        $latestTag = $tagObj.Trim()

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
        [string]$ArchivePath,
        [string]$ToolPath = $null
    )

    # Use the provided tool path or resolve it internally if not provided.
    # This decouples the function from the global $Script scope.
    $7z = if (-not [string]::IsNullOrEmpty($ToolPath)) { $ToolPath } else { Find-7ZipExecutable }

    if ([string]::IsNullOrEmpty($7z)) {
        Write-Log "7-Zip not found. Using Compress-Archive instead." "WARN"

        $parentDir = Split-Path $ArchivePath -Parent
        if (-not (Test-Path $parentDir)) {
            New-Item -ItemType Directory -Path $parentDir -Force | Out-Null
        }

        try {
            # Compress-Archive requires strict path handling
            $filesToCompress = Get-ChildItem -Path $SourceDir -Force
            Compress-Archive -Path $filesToCompress.FullName -DestinationPath $ArchivePath -CompressionLevel Optimal -Force
            return [CommandResult]::Ok("Archive created using Compress-Archive", $ArchivePath)
        }
        catch {
            return [CommandResult]::Fail("Compress-Archive failed: $_")
        }
    }

    # Use the specific executable found
    $sevenZipArgs = "a -t7z -mx=3 `"$ArchivePath`" `"$SourceDir\*`""
    $result = Invoke-ExternalCommand -ExecutablePath $7z -Arguments $sevenZipArgs

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
            $depjson = Get-Content $depjsonPath -Raw | ConvertFrom-Json
            $modified = $false

            # The createdump.exe reference is typically found within the native assets of a runtime package.
            # This logic navigates the complex structure to find and remove it safely.
            if ($depjson.PSObject.Properties['targets']) {
                foreach ($targetProperty in $depjson.targets.PSObject.Properties) {
                    $libraries = $targetProperty.Value
                    foreach ($libraryProperty in $libraries.PSObject.Properties) {
                        $library = $libraryProperty.Value

                        # Safely check for 'native' property existence
                        if ($library.PSObject.Properties['native'] -and $library.native) {
                            $nativeAssets = $library.native

                            # Check if native assets contain createdump.exe
                            if ($nativeAssets.PSObject.Properties['createdump.exe']) {
                                $nativeAssets.PSObject.Properties.Remove('createdump.exe')
                                Write-Log "Removed 'createdump.exe' from native assets under '$($libraryProperty.Name)'"
                                $modified = $true

                                # If the 'native' object is now empty, remove it for cleanliness.
                                if ($nativeAssets.PSObject.Properties.Count -eq 0) {
                                    $library.PSObject.Properties.Remove('native')
                                }
                            }
                        }
                    }
                }
            }

            if ($modified) {
                # Convert back to JSON and write to file. Depth is important for complex objects.
                $depjson | ConvertTo-Json -Depth 100 | Set-Content -Path $depjsonPath -Encoding UTF8
                Write-Log "Createdump reference removed from deps.json"
            }
            else {
                Write-Log "No createdump reference found in deps.json to remove."
            }
        }
        catch {
            Write-Log "Failed to process deps.json for createdump reference: $_" "ERROR"
        }
    }

    $createDumpExe = Join-Path $Path "createdump.exe"
    if (Test-Path $createDumpExe) {
        Remove-Item -Path $createDumpExe -Force -ErrorAction SilentlyContinue
        Write-Log "Createdump.exe removed"
    }
}