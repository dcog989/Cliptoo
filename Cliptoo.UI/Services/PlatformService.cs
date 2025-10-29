using System.Windows;
using Cliptoo.Core;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Native;

namespace Cliptoo.UI.Services
{
    internal class PlatformService : IPlatformService
    {
        private readonly CliptooController _controller;
        private readonly ISettingsService _settingsService;
        private GlobalHotkey? _globalHotkey;
        private string? _currentHotkey;

        public event EventHandler? HotkeyPressed;

        public PlatformService(CliptooController controller, ISettingsService settingsService)
        {
            _controller = controller;
            _settingsService = settingsService;
            _settingsService.SettingsChanged += OnSettingsChanged;
        }

        public void Initialize(IntPtr windowHandle)
        {
            _currentHotkey = _settingsService.Settings.Hotkey;

            _globalHotkey = new GlobalHotkey(windowHandle);
            if (!_globalHotkey.Register(_currentHotkey))
            {
                var message = $"Failed to register the global hotkey '{_currentHotkey}'. It may be in use by another application. You can set a new one in Settings via the tray icon.";
                LogManager.LogWarning(message);
                MessageBox.Show(message, "Cliptoo Warning", MessageBoxButton.OK, MessageBoxImage.Warning);
            }
            else
            {
                _globalHotkey.HotkeyPressed += (s, e) => HotkeyPressed?.Invoke(this, EventArgs.Empty);
            }
            LogManager.LogDebug("Global hotkey registered.");

            _controller.ClipboardMonitor.Start(windowHandle);
            LogManager.LogDebug("Clipboard monitor started.");
        }

        public void OnClipboardUpdate()
        {
            _controller.ClipboardMonitor.ProcessSystemUpdate();
        }

        public void OnHotkeyPressed()
        {
            _globalHotkey?.OnHotkeyPressed();
        }

        private void OnSettingsChanged(object? sender, EventArgs e)
        {
            var settings = _settingsService.Settings;
            LogManager.Configure(settings.LoggingLevel, settings.LogRetentionDays);
            if (_globalHotkey != null && _currentHotkey != settings.Hotkey)
            {
                _currentHotkey = settings.Hotkey;
                _globalHotkey.Register(_currentHotkey);
                LogManager.LogInfo($"Global hotkey re-registered to: {_currentHotkey}");
            }
        }

        public void Dispose()
        {
            _globalHotkey?.Dispose();
            _settingsService.SettingsChanged -= OnSettingsChanged;
            GC.SuppressFinalize(this);
        }
    }
}