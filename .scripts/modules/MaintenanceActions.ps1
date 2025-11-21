# Builder Toolbox - Maintenance Actions

function Remove-BuildOutput {
    [CmdletBinding(SupportsShouldProcess = $true)]
    param(
        [switch]$NoConfirm,
        [switch]$Quiet
    )

    if (-not $NoConfirm) {
        if (-not (Confirm-IdeShutdown -Action "Clean Solution")) { return }
        if (-not (Confirm-ProcessTermination -Action "Clean")) { return }
    }

    if (-not $Quiet) {
        Write-Log "Cleaning build files..." "CONSOLE"
    }

    # Method 1: Use dotnet clean for proper .NET cleanup
    if ($PSCmdlet.ShouldProcess("$($Script:SolutionFile)", "Run 'dotnet clean'")) {
        if (-not $Quiet) { Write-Log "Running 'dotnet clean'..." }
        $cleanResult = Invoke-DotnetCommand -Command "clean" -Arguments "`"$Script:SolutionFile`" -c Release -v minimal" -IgnoreErrors
        if ($cleanResult.Success -and -not $Quiet) {
            Write-Log "Dotnet clean completed" "SUCCESS"
        }
    }

    # Method 2: Manual cleanup for comprehensive removal
    $searchPaths = @($Script:SolutionRoot)

    $buildDirs = @()
    foreach ($searchPath in $searchPaths) {
        if (-not $Quiet) { Write-Log "Searching for build directories in: $searchPath" }

        # use specific filters for 'bin' and 'obj' separately for higher reliability than -Include
        $binDirs = Get-ChildItem -Path $searchPath -Filter "bin" -Directory -Recurse -Force -ErrorAction SilentlyContinue
        $objDirs = Get-ChildItem -Path $searchPath -Filter "obj" -Directory -Recurse -Force -ErrorAction SilentlyContinue
        
        # Ensure we don't add nulls to the array if Get-ChildItem returns nothing
        $potentialDirs = @()
        if ($binDirs) { $potentialDirs += $binDirs }
        if ($objDirs) { $potentialDirs += $objDirs }

        foreach ($dir in $potentialDirs) {
            # Double check dir object is valid
            if (-not $dir) { continue }

            $parentDir = $dir.Parent.FullName
            # Safety check: Ensure parent directory contains a project file
            $hasProjectFile = Get-ChildItem -Path $parentDir -Filter "*.*proj" -File -ErrorAction SilentlyContinue | Select-Object -First 1

            if ($hasProjectFile) {
                $buildDirs += $dir
            }
        }
    }

    # Remove duplicates and exclude any paths that might be in use
    $buildDirs = $buildDirs | Sort-Object -Property FullName -Unique | Where-Object {
        try {
            # Test if directory can be accessed (not locked)
            Get-ChildItem $_.FullName -ErrorAction Stop | Out-Null
            $true
        }
        catch {
            if (-not $Quiet) { Write-Log "Skipping locked directory: $($_.FullName)" "WARN" }
            $false
        }
    }

    if ($buildDirs.Count -gt 0) {
        if (-not $Quiet) { Write-Log "Found $($buildDirs.Count) build directories to remove" }
        $counter = 0

        foreach ($dir in $buildDirs) {
            $counter++

            if ($PSCmdlet.ShouldProcess($dir.FullName, "Delete directory recursively")) {
                try {
                    # Retry logic for stubborn locks
                    Remove-Item -Path $dir.FullName -Recurse -Force -ErrorAction Stop
                    Write-Log "Removed: $($dir.FullName)" "DEBUG"
                }
                catch {
                    Write-Log "Failed to remove: $($dir.FullName) - $($_.Exception.Message)" "WARN"
                }
            }
        }

        if (-not $WhatIfPreference -and -not $Quiet) {
            Write-Log "Removed $($buildDirs.Count) build directories" "SUCCESS"
        }
    }
    else {
        if (-not $Quiet) { Write-Log "No 'bin' or 'obj' directories found to clean." }
    }

    # Cleanup Velopack releases if enabled
    if ($Script:UseVelopack) {
        $releaseDir = Join-Path $Script:SolutionRoot "Releases"
        if (Test-Path $releaseDir) {
            if ($PSCmdlet.ShouldProcess($releaseDir, "Delete Velopack releases")) {
                if (-not $Quiet) { Write-Log "Removing Velopack releases directory..." }
                try {
                    Remove-Item -Path $releaseDir -Recurse -Force -ErrorAction Stop
                    if (-not $Quiet) { Write-Log "Removed Velopack releases directory." "SUCCESS" }
                }
                catch {
                    Write-Log "Failed to remove Velopack releases: $($_.Exception.Message)" "WARN"
                }
            }
        }
    }

    # Cleanup any packages directories
    $packageDirs = Get-ChildItem -Path $Script:SolutionRoot -Include "packages", "package", "publish" -Directory -Recurse -ErrorAction SilentlyContinue
    foreach ($pkgDir in $packageDirs) {
        if ($PSCmdlet.ShouldProcess($pkgDir.FullName, "Delete package directory")) {
            try {
                Remove-Item -Path $pkgDir.FullName -Recurse -Force -ErrorAction Stop
                Write-Log "Removed package directory: $($pkgDir.FullName)" "DEBUG"
            }
            catch {
                Write-Log "Failed to remove package directory: $($pkgDir.FullName)" "WARN"
            }
        }
    }

    Clear-BuildVersionCache
    if (-not $Quiet) { Write-Log "Cleanup completed." "SUCCESS" }
}

function Update-VersionNumber {
    [CmdletBinding(SupportsShouldProcess = $true)]
    param()

    if (-not (Test-ProjectFilesExist)) { return }
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

    if ($newVersion -notmatch '^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?$') {
        Write-Log "Invalid version format. Use Semantic Versioning (e.g., 1.2.3 or 1.2.3-beta1)." "ERROR"
        return
    }

    try {
        Add-Type -AssemblyName System.Xml.Linq

        # Load with PreserveWhitespace to keep existing indentation
        $doc = [System.Xml.Linq.XDocument]::Load($Script:MainProjectFile, [System.Xml.Linq.LoadOptions]::PreserveWhitespace)
        $ns = $doc.Root.Name.Namespace

        # Find existing Version element regardless of where it is
        $versionElement = $doc.Descendants($ns + "Version") | Select-Object -First 1

        if ($versionElement) {
            $versionElement.Value = $newVersion
        }
        else {
            # No Version tag exists, append to the first PropertyGroup
            $propertyGroup = $doc.Descendants($ns + "PropertyGroup") | Select-Object -First 1
            if ($propertyGroup) {
                # Construct new element
                $newEl = [System.Xml.Linq.XElement]::new($ns + "Version", $newVersion)

                # Attempt to mimic indentation of the last element in the group
                if ($propertyGroup.LastNode.NodeType -eq [System.Xml.XmlNodeType]::Text) {
                    $propertyGroup.Add($propertyGroup.LastNode) # Copy whitespace
                }
                $propertyGroup.Add($newEl)
            }
            else {
                throw "No PropertyGroup found in project file to add Version."
            }
        }

        if ($PSCmdlet.ShouldProcess($Script:MainProjectFile, "Update version to $newVersion")) {
            $doc.Save($Script:MainProjectFile)
            $Script:BuildVersion = $newVersion
            Write-Log "Version updated to $newVersion in $($Script:MainProjectFile)" "SUCCESS"
        }
    }
    catch {
        Write-Log "Failed to update version: $_" "ERROR"
        Write-Host "Details: $($_.Exception.Message)" -ForegroundColor DarkGray
    }
}

function Invoke-UnitTests {
    if (-not (Test-ProjectFilesExist)) { return }
    Write-Log "Running unit tests for solution..." "CONSOLE"

    $result = Invoke-DotnetCommand -Command "test" -Arguments "`"$Script:SolutionFile`""

    if ($result.Success) {
        Write-Log "Test run successful. Check log for details (e.g., if no tests were found)." "SUCCESS"
    }
    else {
        Write-Log "One or more tests failed. Check the log file for details." "ERROR"
        Invoke-ItemSafely -Path (Get-LogFile) -ItemType "Log file"
    }
}