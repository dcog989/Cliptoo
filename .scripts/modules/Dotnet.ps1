# Builder Toolbox - Dotnet and Command Execution

# --- Prerequisite Check ---
function Test-DotNetVersion {
    if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
        $msg = "'dotnet.exe' not found. Ensure .NET $($Script:RequiredDotNetVersion) SDK is installed and in your PATH."
        return [CommandResult]::Fail($msg)
    }

    $versionOutput = (dotnet --version 2>$null).Trim()

    # Parse installed version (remove preview suffixes for comparison)
    $cleanInstalled = $versionOutput -replace '-.*$', ''

    try {
        $installedVersion = [version]$cleanInstalled

        # Handle single integer input for requirement (e.g. "9" -> "9.0")
        $reqString = if ($Script:RequiredDotNetVersion -notmatch "\.") { "$($Script:RequiredDotNetVersion).0" } else { $Script:RequiredDotNetVersion }
        $requiredVersion = [version]$reqString

        # Allow if Major version is greater or equal to requirement
        if ($installedVersion.Major -ge $requiredVersion.Major) {
            $Script:SdkVersion = $versionOutput
            return [CommandResult]::Ok("Found compatible version $versionOutput (Required: $Script:RequiredDotNetVersion+)", $versionOutput)
        }
        else {
            $msg = "Installed .NET SDK ($versionOutput) is older than required ($Script:RequiredDotNetVersion). Please install a newer SDK."
            return [CommandResult]::Fail($msg)
        }
    }
    catch {
        # Fallback to regex if parsing fails (legacy behavior)
        if ($versionOutput -match "^$($Script:RequiredDotNetVersion)\.") {
            $Script:SdkVersion = $versionOutput
            return [CommandResult]::Ok("Found version $versionOutput", $versionOutput)
        }

        $msg = "Version check failed. Required: $Script:RequiredDotNetVersion, Found: $versionOutput. Error: $_"
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
    $streamWriter = $null

    try {
        $process = [System.Diagnostics.Process]::Start($processInfo)

        # Use StreamWriter for continuous logging instead of Add-Content
        # This avoids opening/closing the file handle for every single line of output.
        $streamWriter = [System.IO.File]::AppendText($logFile)
        $streamWriter.AutoFlush = $true

        # We'll capture the last few lines for error reporting if needed
        $errorBuffer = [System.Collections.Generic.Queue[string]]::new()
        $maxErrorLines = 20

        while (-not $process.HasExited) {
            # Read standard output
            while (-not $process.StandardOutput.EndOfStream) {
                $line = $process.StandardOutput.ReadLine()
                if ($line) {
                    $streamWriter.WriteLine($line)
                }
            }

            # Read standard error
            while (-not $process.StandardError.EndOfStream) {
                $line = $process.StandardError.ReadLine()
                if ($line) {
                    $logLine = "ERROR: $line"
                    $streamWriter.WriteLine($logLine)

                    # Keep recent errors for return message
                    $errorBuffer.Enqueue($line)
                    if ($errorBuffer.Count -gt $maxErrorLines) { $errorBuffer.Dequeue() | Out-Null }
                }
            }

            Start-Sleep -Milliseconds 100
        }

        # Flush any remaining output after exit
        $remainingOut = $process.StandardOutput.ReadToEnd()
        if ($remainingOut) { $streamWriter.Write($remainingOut) }

        $remainingErr = $process.StandardError.ReadToEnd()
        if ($remainingErr) {
            $streamWriter.WriteLine("ERROR: $remainingErr")
            foreach ($line in ($remainingErr -split "`n")) {
                $errorBuffer.Enqueue($line.Trim())
                if ($errorBuffer.Count -gt $maxErrorLines) { $errorBuffer.Dequeue() | Out-Null }
            }
        }

        $process.WaitForExit()

        if ($process.ExitCode -eq 0 -or $IgnoreErrors) {
            return [CommandResult]::Ok("External command successful", @{ ExitCode = $process.ExitCode })
        }
        else {
            $recentErrors = $errorBuffer.ToArray() -join "`n"
            $failureMsg = "Process exited with code $($process.ExitCode). See log for full details."
            if (-not [string]::IsNullOrWhiteSpace($recentErrors)) {
                $failureMsg += "`nLast errors:`n$recentErrors"
            }

            return [CommandResult]::Fail($failureMsg, $process.ExitCode)
        }
    }
    catch {
        return [CommandResult]::Fail("Error starting process: $($_.Exception.Message)")
    }
    finally {
        if ($streamWriter) {
            $streamWriter.Dispose()
        }
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
            Add-Type -AssemblyName System.Xml.Linq
            $doc = [System.Xml.Linq.XDocument]::Load($Script:MainProjectFile)
            $ns = $doc.Root.Name.Namespace

            $assemblyNameNode = $doc.Descendants($ns + "AssemblyName") | Select-Object -First 1

            if ($null -ne $assemblyNameNode -and -not [string]::IsNullOrWhiteSpace($assemblyNameNode.Value)) {
                $appName = $assemblyNameNode.Value.Trim()
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
        Add-Type -AssemblyName System.Xml.Linq
        $doc = [System.Xml.Linq.XDocument]::Load($Script:MainProjectFile)
        $ns = $doc.Root.Name.Namespace

        $versionNode = $doc.Descendants($ns + "Version") | Select-Object -First 1

        if ($null -ne $versionNode) {
            return [CommandResult]::Ok("Version retrieved", $versionNode.Value.Trim())
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