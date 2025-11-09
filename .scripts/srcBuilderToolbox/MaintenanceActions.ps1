# Builder Toolbox - Maintenance Actions

function Remove-BuildOutput {
    param([switch]$NoConfirm)

    if (-not $NoConfirm) {
        if (-not (Confirm-Ideshutdown -Action "Clean Solution")) { return }
        if (-not (Confirm-ProcessTermination -Action "Clean")) { return }
    }

    Write-Log "Attempting to clean build files using 'dotnet clean'..." "CONSOLE"

    # 1. Use the native dotnet clean command, which correctly handles project structure dependencies.
    $cleanResult = Invoke-DotnetCommand -Command "clean" -Arguments "`"$Script:SolutionFile`""
    
    if (-not $cleanResult.Success) {
        Write-Log "The 'dotnet clean' command failed. Proceeding with manual cleanup (this may be due to file locks)." "WARN"
    }

    # 2. Manual cleanup fallback: Find all project folders and forcefully remove bin/obj.
    $projectFiles = Get-ChildItem -Path $Script:SolutionRoot -Filter "*.csproj" -Recurse -File -ErrorAction SilentlyContinue
    $projectParentDirs = $projectFiles | Select-Object -ExpandProperty DirectoryName | Get-Unique

    $buildDirs = @()
    if ($projectParentDirs.Count -gt 0) {
        Write-Log "Found $($projectParentDirs.Count) project directories. Searching for 'bin' and 'obj' folders within them for forced removal."
        # Use simple Join-Path/Test-Path to build the list of directories to remove
        foreach ($dirPath in $projectParentDirs) {
            $binPath = Join-Path $dirPath "bin"
            $objPath = Join-Path $dirPath "obj"
            if (Test-Path $binPath) { $buildDirs += $binPath }
            if (Test-Path $objPath) { $buildDirs += $objPath }
        }
    }

    if ($buildDirs.Count -gt 0) {
        $counter = 0
        $total = $buildDirs.Count
        Write-Log "Manually removing $($buildDirs.Count) directories..."
        foreach ($dir in $buildDirs) {
            $counter++
            Write-Progress -Activity "Cleaning build directories (Force)" -Status "Removing $($dir | Split-Path -Leaf)" -PercentComplete (($counter / $total) * 100)
            
            # This is the key: forcibly delete the locked directories.
            Remove-Item -Path $dir -Recurse -Force -ErrorAction SilentlyContinue
        }
        # Ensure progress bar is completed AND cleared from display
        Write-Progress -Activity "Cleaning build directories (Force)" -Completed
        # Small delay to let PowerShell process the completion
        Start-Sleep -Milliseconds 50
        Write-Log "Manually removed $($buildDirs.Count) directories."
    }
    else {
        Write-Log "No 'bin' or 'obj' directories found after 'dotnet clean' to manually clean."
    }

    # 3. Clean Velopack releases directory if applicable
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