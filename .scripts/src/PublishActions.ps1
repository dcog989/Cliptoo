# Builder Toolbox - Publishing and Packaging Actions

function Publish-Portable {
    if (-not (Test-ProjectFilesExist)) { return }
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

    if (-not $Script:SevenZipPath) { $Script:SevenZipPath = Find-7ZipExecutable }

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

function New-ProductionPackage {
    if (-not (Test-ProjectFilesExist)) { return }
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
        if (-not $Script:SevenZipPath) { $Script:SevenZipPath = Find-7ZipExecutable }

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
