function Initialize-PublishContext {
    param(
        [string]$ActionName
    )
    if (-not (Test-ProjectFilesExist)) { return $null }
    if (-not (Confirm-IdeShutdown -Action $ActionName)) { return $null }
    if (-not (Confirm-ProcessTermination -Action $ActionName)) { return $null }

    $version = Get-CachedBuildVersion
    if ([string]::IsNullOrEmpty($version)) {
        Write-Log "Could not determine package version from csproj. Please set it first." "ERROR"
        return $null
    }

    $mainProjectDir = Split-Path -Path $Script:MainProjectFile -Parent
    $baseOutputDir = Join-Path $mainProjectDir "bin\Release\$($Script:TargetFramework)\$($Script:PublishRuntimeId)"

    # Pre-clean the solution before building
    # Added -Quiet to prevent progress bar ghosting/spam during publish operations
    Remove-BuildOutput -NoConfirm -Quiet

    return [PSCustomObject]@{
        BaseOutputDir = $baseOutputDir
        PublishDir    = (Join-Path $baseOutputDir "publish")
        Version       = $version
    }
}

function Invoke-PostPublishCleanup {
    param(
        [string]$PublishDir
    )
    Write-Log "Post-build processing..." "CONSOLE"

    if ($Script:RemoveCreateDump) {
        Remove-CreateDumpReference -Path $PublishDir
    }

    if ($Script:RemoveXmlFiles) {
        $keepPatterns = if ($Script:XmlKeepPatterns) { $Script:XmlKeepPatterns } else { @() }

        $xmlFiles = Get-ChildItem -Path $PublishDir -Filter "*.xml" -Recurse -File -ErrorAction SilentlyContinue
        $removedCount = 0

        foreach ($file in $xmlFiles) {
            $shouldKeep = $false
            foreach ($pattern in $keepPatterns) {
                if ($file.Name -like $pattern) {
                    $shouldKeep = $true
                    break
                }
            }

            if (-not $shouldKeep) {
                Remove-Item -Path $file.FullName -Force -ErrorAction SilentlyContinue
                $removedCount++
            }
        }
        Write-Log "Removed $removedCount documentation files (*.xml)."
    }

    $removedPdbCount = Remove-FilesByPattern -Path $PublishDir -Patterns @("*.pdb")
    Write-Log "Removed $removedPdbCount debug symbols (*.pdb)."
}

function Invoke-PublishBuild {
    param(
        [string]$OutputDir,
        [string]$Arguments,
        [string]$ActionDescription
    )

    # Ensure clean output directory
    if (Test-Path $OutputDir) {
        Remove-Item -Recurse -Force $OutputDir -ErrorAction SilentlyContinue
    }
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

    Write-Log "$ActionDescription..." "CONSOLE"

    $result = Invoke-DotnetCommand -Command "publish" -Arguments $Arguments
    if (-not $result.Success) {
        Write-Log "Publish failed: $($result.Message)" "ERROR"
        return $false
    }

    Invoke-PostPublishCleanup -PublishDir $OutputDir
    return $true
}

function Invoke-ArchivePackaging {
    param(
        [string]$SourceDir,
        [string]$OutputDir,
        [string]$ArchiveFormat,
        [string]$Version,
        [bool]$AddPortableMarker
    )

    if ($AddPortableMarker) {
        Write-Log "Adding portable mode marker..."
        $portableMarkerPath = Join-Path $SourceDir $Script:PortableMarkerFile
        Set-Content -Path $portableMarkerPath -Value "This file enables portable mode. Do not delete."
    }

    $sevenZipPath = Find-7ZipExecutable

    $archiveFileName = [string]::Format($ArchiveFormat, $Script:PackageTitle, $Version)
    $destinationArchive = Join-Path $OutputDir $archiveFileName

    if (Test-Path $destinationArchive) {
        Remove-Item $destinationArchive -Force
    }

    $archiveResult = Compress-With7Zip `
        -SourceDir $SourceDir `
        -ArchivePath $destinationArchive `
        -ToolPath $sevenZipPath

    if (-not $archiveResult.Success) {
        Write-Log "Archiving failed: $($archiveResult.Message)" "ERROR"
        return $false
    }

    Write-Log "Archive created: $destinationArchive" "SUCCESS"
    return $true
}

function Publish-Portable {
    $context = Initialize-PublishContext -ActionName "Publish Portable Package"
    if (-not $context) { return }

    $packageDir = Join-Path $context.BaseOutputDir "packages"
    if (-not (Test-Path $packageDir)) { New-Item -ItemType Directory -Path $packageDir -Force | Out-Null }

    # 1. Build
    $buildArgs = "`"$Script:MainProjectFile`" -c Release -r $Script:PublishRuntimeId --self-contained true -o `"$($context.PublishDir)`""
    $success = Invoke-PublishBuild -OutputDir $context.PublishDir -Arguments $buildArgs -ActionDescription "Building Portable Package"
    if (-not $success) { return }

    # 2. Package
    $pkgSuccess = Invoke-ArchivePackaging `
        -SourceDir $context.PublishDir `
        -OutputDir $packageDir `
        -ArchiveFormat $Script:PortableArchiveFormat `
        -Version $context.Version `
        -AddPortableMarker $true

    if ($pkgSuccess) {
        New-ChangelogFromGit -OutputDir $packageDir
        Invoke-ItemSafely -Path $packageDir -ItemType "Output directory"
    }
}

function New-ProductionPackage {
    $context = Initialize-PublishContext -ActionName "Production Build"
    if (-not $context) { return }

    if ($Script:UseVelopack) {
        # --- Velopack Build Path ---
        $releaseDir = Join-Path $Script:SolutionRoot "Releases"

        # 1. Build (Velopack needs the raw files first)
        $publishArgs = "`"$Script:MainProjectFile`" -c Release -r $Script:PublishRuntimeId --self-contained true -o `"$($context.PublishDir)`""
        $success = Invoke-PublishBuild -OutputDir $context.PublishDir -Arguments $publishArgs -ActionDescription "Publishing for Velopack"
        if (-not $success) { return }

        # Clean release dir for fresh artifacts
        if (Test-Path $releaseDir) { Remove-Item -Recurse -Force $releaseDir -ErrorAction SilentlyContinue }

        # 2. Pack with Velopack
        $iconArg = if (Test-Path $Script:PackageIconPath) { "--icon `"$($Script:PackageIconPath)`"" } else { "" }
        $channelArg = if (-not [string]::IsNullOrEmpty($Script:VelopackChannelName)) { "-c $($Script:VelopackChannelName)" } else { "" }

        Write-Log "Packaging with Velopack..." "CONSOLE"
        $velopackArgs = "pack --packId `"$($Script:PackageId)`" --packId $($Script:PackageId) --packVersion $($context.Version) --packDir `"$($context.PublishDir)`" -o `"$releaseDir`" $iconArg $channelArg --verbose"

        Write-Log "DIAGNOSTIC - Executing command: vpk $velopackArgs"
        $packResult = Invoke-ExternalCommand -ExecutablePath "vpk" -Arguments $velopackArgs

        if (-not $packResult.Success) {
            Write-Log "Velopack packaging failed: $($packResult.Message)" "ERROR"
            return
        }

        # 3. Rename Artifacts
        Write-Log "Renaming release artifacts..."
        try {
            $setupFile = Get-ChildItem -Path $releaseDir -Filter "*Setup.exe" | Select-Object -First 1
            if ($setupFile) {
                $newSetupName = "$($Script:PackageTitle)-Windows-x64-Setup-v$($context.Version).exe"
                Rename-Item -Path $setupFile.FullName -NewName $newSetupName
                Write-Log "Renamed installer to $newSetupName"
            }

            $portableFile = Get-ChildItem -Path $releaseDir -Filter "*-portable.zip" | Select-Object -First 1
            if ($portableFile) {
                $newPortableName = "$($Script:PackageTitle)-Windows-x64-Portable-v$($context.Version).zip"
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
        # --- 7-Zip Build Path (Legacy) ---
        $releaseDir = Join-Path $context.BaseOutputDir "release_packages"
        $stagingDir = Join-Path $context.BaseOutputDir "production_staging"

        if (Test-Path $releaseDir) { Remove-Item -Recurse -Force $releaseDir -ErrorAction SilentlyContinue }
        New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null

        # 1. Build (SingleFile, ReadyToRun)
        $publishArgs = "`"$Script:MainProjectFile`" -c Release -o `"$stagingDir`" --self-contained true -p:PublishSingleFile=true -p:PublishReadyToRun=true"
        $success = Invoke-PublishBuild -OutputDir $stagingDir -Arguments $publishArgs -ActionDescription "Building Release Package"
        if (-not $success) { return }

        # 2. Create Standard Package
        Invoke-ArchivePackaging `
            -SourceDir $stagingDir `
            -OutputDir $releaseDir `
            -ArchiveFormat $Script:StandardArchiveFormat `
            -Version $context.Version `
            -AddPortableMarker $false

        # 3. Create Portable Package (Reuse staging, add marker)
        Invoke-ArchivePackaging `
            -SourceDir $stagingDir `
            -OutputDir $releaseDir `
            -ArchiveFormat $Script:PortableArchiveFormat `
            -Version $context.Version `
            -AddPortableMarker $true

        New-ChangelogFromGit -OutputDir $releaseDir

        # Cleanup staging
        if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir -ErrorAction SilentlyContinue }

        Write-Log "Full release package created at: $releaseDir" "SUCCESS"
        Invoke-ItemSafely -Path $releaseDir -ItemType "Release directory"
    }
}