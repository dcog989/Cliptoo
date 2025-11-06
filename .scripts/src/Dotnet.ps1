# Builder Toolbox - Dotnet and Command Execution

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

    # Normalize to lower case, which is often what Get-Process expects without extension
    $Script:AppNameCache = $appName
    return $appName
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

function Get-CachedBuildVersion {
    if ($null -eq $Script:BuildVersion) {
        if (-not (Test-Path $Script:MainProjectFile)) {
            return $null # Silently return null if the project file doesn't exist.
        }

        $versionResult = Get-BuildVersion
        if ($versionResult.Success) {
            $Script:BuildVersion = $versionResult.Data
            Write-Log "Build version cached: $Script:BuildVersion" "DEBUG"
        }
        else {
            # Log an error only if we failed to parse an *existing* file.
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