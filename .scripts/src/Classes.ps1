# Builder Toolbox - Data Model

# --- Standardized Result Class ---
class CommandResult {
    [bool]$Success
    [string]$Message
    [int]$ExitCode
    [object]$Data

    CommandResult([bool]$success, [string]$message, [int]$exitCode, [object]$data) {
        $this.Success = $success
        $this.Message = $message
        $this.ExitCode = $exitCode
        $this.Data = $data
    }

    static [CommandResult]Ok([string]$message, [object]$data) {
        return [CommandResult]::new($true, $message, 0, $data)
    }

    static [CommandResult]Ok([string]$message) {
        return [CommandResult]::new($true, $message, 0, $null)
    }

    static [CommandResult]Fail([string]$message, [int]$exitCode) {
        return [CommandResult]::new($false, $message, $exitCode, $null)
    }

    static [CommandResult]Fail([string]$message) {
        return [CommandResult]::new($false, $message, -1, $null)
    }
}