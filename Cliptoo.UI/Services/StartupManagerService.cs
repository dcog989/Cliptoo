using System.Diagnostics;
using System.IO;
using System.Security;
using Cliptoo.Core.Logging;
using Microsoft.Win32;

namespace Cliptoo.UI.Services
{
    internal class StartupManagerService : IStartupManagerService
    {
        private const string AppName = "Cliptoo";
        private const string StartupApprovedKeyPath = @"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
        private readonly string _exePath;
        private readonly RegistryKey? _startupKey;

        public StartupManagerService()
        {
            try
            {
                _startupKey = Registry.CurrentUser.OpenSubKey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run", true);

                // Resolve the executable path.
                var currentProcessPath = Process.GetCurrentProcess().MainModule?.FileName;

                if (!string.IsNullOrEmpty(currentProcessPath))
                {
                    // Check if we are running in a Velopack versioned directory (e.g. ...\Cliptoo\app-1.0.0\Cliptoo.UI.exe)
                    var directory = Path.GetDirectoryName(currentProcessPath);
                    if (directory != null && Path.GetFileName(directory).StartsWith("app-", StringComparison.OrdinalIgnoreCase))
                    {
                        var rootDir = Path.GetDirectoryName(directory);
                        if (rootDir != null)
                        {
                            // Look for the stable shim in the root folder (e.g. ...\Cliptoo\Cliptoo.exe)
                            // This file persists across updates and redirects to the latest version.
                            var shimPath = Path.Combine(rootDir, "Cliptoo.exe");
                            if (File.Exists(shimPath))
                            {
                                _exePath = shimPath;
                                LogManager.LogDebug($"StartupManager: Detected Velopack structure. Using shim path for startup: {_exePath}");
                            }
                            else
                            {
                                _exePath = currentProcessPath;
                            }
                        }
                        else
                        {
                            _exePath = currentProcessPath;
                        }
                    }
                    else
                    {
                        _exePath = currentProcessPath;
                    }
                }
                else
                {
                    _exePath = string.Empty;
                }
            }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or SecurityException or UnauthorizedAccessException or InvalidOperationException or IOException)
            {
                LogManager.LogError($"Failed to initialize StartupManagerService: {ex.Message}");
                _exePath = string.Empty;
            }
        }

        public void SetStartup(bool isEnabled)
        {
            if (_startupKey == null)
            {
                LogManager.LogWarning("Cannot set startup: Registry key is null.");
                return;
            }

            if (string.IsNullOrEmpty(_exePath))
            {
                LogManager.LogWarning("Cannot set startup: Executable path could not be determined.");
                return;
            }

            try
            {
                if (isEnabled)
                {
                    var value = $"\"{_exePath}\"";
                    _startupKey.SetValue(AppName, value);

                    // Fix for "Off" state in Windows Startup Apps:
                    // If Windows has flagged this app as disabled in the "StartupApproved" key,
                    // simply setting the Run key won't re-enable it. We must clear the specific
                    // entry in StartupApproved to reset the state to "Enabled" (default).
                    try
                    {
                        using var approvedKey = Registry.CurrentUser.OpenSubKey(StartupApprovedKeyPath, true);
                        if (approvedKey != null && approvedKey.GetValue(AppName) != null)
                        {
                            approvedKey.DeleteValue(AppName);
                            LogManager.LogDebug("StartupManager: Reset StartupApproved entry to force enable in Windows settings.");
                        }
                    }
                    catch (Exception ex) when (ex is SecurityException or UnauthorizedAccessException or IOException)
                    {
                        // Log but don't fail the whole operation; this is an enhancement, not critical.
                        LogManager.LogWarning($"StartupManager: Failed to reset StartupApproved key: {ex.Message}");
                    }

                    LogManager.LogDebug($"Startup registry key set to: {value}");
                }
                else
                {
                    _startupKey.DeleteValue(AppName, false);
                    LogManager.LogDebug("Startup registry key deleted.");
                }
            }
            catch (Exception ex) when (ex is SecurityException or UnauthorizedAccessException or IOException)
            {
                LogManager.LogError($"Failed to change startup registry key: {ex.Message}");
            }
        }

        public bool IsStartupEnabled()
        {
            if (_startupKey == null) return false;
            try
            {
                return _startupKey.GetValue(AppName) != null;
            }
            catch (Exception ex) when (ex is SecurityException or UnauthorizedAccessException or IOException)
            {
                LogManager.LogError($"Failed to read startup registry key: {ex.Message}");
                return false;
            }
        }
    }
}