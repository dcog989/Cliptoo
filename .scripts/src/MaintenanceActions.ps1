# Builder Toolbox - Maintenance Actions

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