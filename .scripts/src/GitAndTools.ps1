# Builder Toolbox - Git and Tool Helpers

function Find-7ZipExecutable {
    # 1. Check PATH
    $sevenZipInPath = Get-Command 7za.exe -ErrorAction SilentlyContinue
    if ($null -ne $sevenZipInPath) {
        Write-Log "Found 7za.exe in PATH: $($sevenZipInPath.Source)"
        return $sevenZipInPath.Source
    }

    # 2. Check repo root
    $sevenZipAtRoot = Join-Path $Script:RepoRoot "7za.exe"
    if (Test-Path $sevenZipAtRoot) {
        Write-Log "Found 7za.exe in repo root: $sevenZipAtRoot"
        return $sevenZipAtRoot
    }

    # 3. Check 7z subdirectory in repo root
    $sevenZipInSubDir = Join-Path $Script:RepoRoot "7z/7za.exe"
    if (Test-Path $sevenZipInSubDir) {
        Write-Log "Found 7za.exe in 7z subdirectory: $sevenZipInSubDir"
        return $sevenZipInSubDir
    }

    # Not found
    Write-Log "7za.exe not found. Will fall back to PowerShell's Compress-Archive for packaging." "WARN"
    return $null
}

function Get-GitInfo {
    # No caching in this file, caching is done in Main for a single run
    $gitInfo = @{ Branch = "N/A"; Commit = "N/A" }

    if (Get-Command git -ErrorAction SilentlyContinue) {
        # First, check if the solution root is a git repository before running other commands.
        git -C $Script:SolutionRoot rev-parse --is-inside-work-tree 2>$null | Out-Null
        if ($LASTEXITCODE -ne 0) {
            return $gitInfo # Not a git repo, so return default info silently.
        }

        try {
            $branchOutput = git -C $Script:SolutionRoot rev-parse --abbrev-ref HEAD 2>$null
            if ($null -ne $branchOutput) {
                $gitInfo.Branch = $branchOutput.Trim()
            }

            $commitOutput = git -C $Script:SolutionRoot rev-parse --short HEAD 2>$null
            if ($null -ne $commitOutput) {
                $gitInfo.Commit = $commitOutput.Trim()
            }

            if ([string]::IsNullOrWhiteSpace($gitInfo.Branch)) { $gitInfo.Branch = "N/A" }
            if ([string]::IsNullOrWhiteSpace($gitInfo.Commit)) { $gitInfo.Commit = "N/A" }
        }
        catch {
            Write-Log "Failed to get Git info: $_" "WARN"
        }
    }
    # If git is not found, we will just return the default values.

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

    if ([string]::IsNullOrEmpty($Script:SevenZipPath)) {
        Write-Log "7-Zip not found. Using Compress-Archive instead." "WARN"

        $parentDir = Split-Path $ArchivePath -Parent
        if (-not (Test-Path $parentDir)) {
            New-Item -ItemType Directory -Path $parentDir -Force | Out-Null
        }

        try {
            # Compress-Archive has a different argument structure for paths/wildcards
            $filesToCompress = Get-ChildItem -Path $SourceDir -Force
            Compress-Archive -Path $filesToCompress.FullName -DestinationPath $ArchivePath -CompressionLevel Optimal -Force
            return [CommandResult]::Ok("Archive created using Compress-Archive", $ArchivePath)
        }
        catch {
            return [CommandResult]::Fail("Compress-Archive failed: $_")
        }
    }

    # 7-Zip argument handling is kept as original for fidelity, but noted as a refactor opportunity
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