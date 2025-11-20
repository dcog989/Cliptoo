using System.Diagnostics;
using Microsoft.Win32;

namespace Cliptoo.UI.Services
{
    internal class StartupManagerService : IStartupManagerService
    {
        private const string AppName = "Cliptoo";
        private readonly string _exePath;
        private readonly RegistryKey? _startupKey;

        public StartupManagerService()
        {
            _exePath = Process.GetCurrentProcess().MainModule?.FileName ?? string.Empty;
            _startupKey = Registry.CurrentUser.OpenSubKey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run", true);
        }

        public void SetStartup(bool isEnabled)
        {
            if (_startupKey == null || string.IsNullOrEmpty(_exePath))
            {
                return;
            }

            try
            {
                if (isEnabled)
                {
                    _startupKey.SetValue(AppName, $"\"{_exePath}\"");
                }
                else
                {
                    _startupKey.DeleteValue(AppName, false);
                }
            }
            catch (Exception ex) when (ex is System.Security.SecurityException or UnauthorizedAccessException)
            {
                // Fails silently if registry access is denied.
            }
        }

        public bool IsStartupEnabled()
        {
            if (_startupKey == null) return false;
            return _startupKey.GetValue(AppName) != null;
        }
    }
}