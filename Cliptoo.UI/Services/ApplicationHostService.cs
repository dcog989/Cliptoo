using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media.Imaging;
using Cliptoo.Core;
using Cliptoo.Core.Database;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Native;
using Cliptoo.Core.Services.Models;
using Cliptoo.UI.ViewModels;
using Cliptoo.UI.Views;
using Microsoft.Data.Sqlite;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Velopack;
using Wpf.Ui;
using Wpf.Ui.Controls;
using Wpf.Ui.Tray;

namespace Cliptoo.UI.Services
{
    internal class ApplicationHostService : IHostedService, IDisposable
    {
        private const int WM_CLIPBOARDUPDATE = 0x031D;
        private const int WM_HOTKEY = 0x0312;
        private const int WM_MOUSEACTIVATE = 0x0021;
        private const int MA_NOACTIVATE = 3;
        private DateTime _lastToggleVisibility = DateTime.MinValue;

        private readonly INotificationService _notificationService;
        private readonly IServiceProvider _serviceProvider;
        private readonly INotifyIconService _notifyIconService;
        private readonly CliptooController _controller;
        private readonly ISettingsService _settingsService;
        private readonly IAppInteractionService _appInteractionService;
        private readonly IClipDataService _clipDataService;
        private readonly IWindowPositioner _windowPositioner;
        private readonly ISnackbarService _snackbarService;
        private readonly IDbManager _dbManager;
        private readonly IThemeService _themeService;
        private MainWindow? _mainWindow;
        private MainViewModel? _mainViewModel;
        private GlobalHotkey? _globalHotkey;
        private HwndSource? _hwndSource;
        private uint _taskbarCreatedMessageId;

        private System.Windows.Controls.MenuItem? _alwaysOnTopMenuItem;
        private System.Windows.Controls.MenuItem? _showHideMenuItem;
        private string? _currentHotkey;

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern uint RegisterWindowMessage(string lpString);

        public ApplicationHostService(
            IServiceProvider serviceProvider,
            INotifyIconService notifyIconService,
            CliptooController controller,
            IWindowPositioner windowPositioner,
            ISnackbarService snackbarService,
            INotificationService notificationService,
            ISettingsService settingsService,
            IAppInteractionService appInteractionService,
            IClipDataService clipDataService,
            IDbManager dbManager,
            IThemeService themeService)
        {
            _serviceProvider = serviceProvider;
            _notifyIconService = notifyIconService;
            _controller = controller;
            _windowPositioner = windowPositioner;
            _snackbarService = snackbarService;
            _notificationService = notificationService;
            _settingsService = settingsService;
            _appInteractionService = appInteractionService;
            _clipDataService = clipDataService;
            _dbManager = dbManager;
            _themeService = themeService;
        }

        public async Task StartAsync(CancellationToken cancellationToken)
        {
            var appVersion = System.Reflection.Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "0.0.0";
            LogManager.LogInfo($"Cliptoo v{appVersion} starting up...");
            LogManager.LogDebug("ApplicationHostService starting...");

            if (_settingsService.Settings.AutoUpdate)
            {
                LogManager.LogInfo("Auto-update is enabled. Checking for updates...");
                try
                {
                    var um = new UpdateManager("https://github.com/dcog989/cliptoo");
                    var updateInfo = await um.CheckForUpdatesAsync().WaitAsync(cancellationToken).ConfigureAwait(false);
                    if (cancellationToken.IsCancellationRequested) return;
                    if (updateInfo != null)
                    {
                        LogManager.LogInfo($"Update found: {updateInfo.TargetFullRelease.Version}. Downloading...");
                        await um.DownloadUpdatesAsync(updateInfo, cancelToken: cancellationToken).ConfigureAwait(false);
                        if (cancellationToken.IsCancellationRequested) return;
                        LogManager.LogInfo("Update downloaded. Applying and restarting...");
                        um.ApplyUpdatesAndRestart(updateInfo);
                        return; // App will restart, so we can exit the startup process.
                    }
                    else
                    {
                        LogManager.LogInfo("No updates found.");
                    }
                }
                catch (Exception ex) when (ex is not OperationCanceledException)
                {
                    LogManager.LogWarning($"Velopack update check failed: {ex.Message}");
                    // Don't rethrow, just log and continue launching the current version.
                }
            }
            else
            {
                LogManager.LogInfo("Auto-update is disabled by user setting.");
            }

            try
            {
                await _dbManager.InitializeAsync().ConfigureAwait(false);
                LogManager.LogDebug("Database initialized successfully.");
                await _controller.InitializeAsync().ConfigureAwait(false);
                await Application.Current.Dispatcher.InvokeAsync(async () =>
                {
                    try
                    {
                        _settingsService.SettingsChanged += OnSettingsChanged;

                        var settings = _settingsService.Settings;
                        _currentHotkey = settings.Hotkey;
                        LogManager.LogDebug("Settings loaded.");

                        _mainWindow = _serviceProvider.GetRequiredService<MainWindow>();
                        _mainWindow.MaxHeight = SystemParameters.WorkArea.Height * 0.9;

                        _mainViewModel = _serviceProvider.GetRequiredService<MainWindow>().DataContext as MainViewModel;
                        if (_mainViewModel != null)
                        {
                            // This await must resume on the UI thread because the subsequent
                            // lines access UI-bound properties.
#pragma warning disable CA2007 // Consider calling ConfigureAwait on the awaited task
                            await _mainViewModel.InitializeAsync();
#pragma warning restore CA2007
                            _mainViewModel.IsAlwaysOnTop = settings.IsAlwaysOnTop;
                            _mainViewModel.InitializeFirstFilter();
                        }

                        _mainWindow.Opacity = 0;
                        _mainWindow.WindowStartupLocation = WindowStartupLocation.Manual;
                        _mainWindow.Left = -9999;
                        _mainWindow.Show();
                        _snackbarService.SetSnackbarPresenter(_mainWindow.SnackbarPresenter);
                        _mainWindow.Hide();
                        _mainWindow.Opacity = 1;

                        var windowInteropHelper = new WindowInteropHelper(_mainWindow);
                        IntPtr handle = windowInteropHelper.EnsureHandle();

                        _themeService.Initialize(_mainWindow);

                        _taskbarCreatedMessageId = RegisterWindowMessage("TaskbarCreated");
                        _hwndSource = HwndSource.FromHwnd(handle);
                        _hwndSource?.AddHook(HwndHook);

                        _globalHotkey = new GlobalHotkey(handle);
                        if (!_globalHotkey.Register(_currentHotkey))
                        {
                            var message = $"Failed to register the global hotkey '{_currentHotkey}'. It may be in use by another application. You can set a new one in Settings via the tray icon.";
                            LogManager.LogWarning(message);
                            System.Windows.MessageBox.Show(message, "Cliptoo Warning", System.Windows.MessageBoxButton.OK, System.Windows.MessageBoxImage.Warning);
                        }
                        else
                        {
                            _globalHotkey.HotkeyPressed += OnHotkeyPressed;
                        }
                        LogManager.LogDebug("Global hotkey registered.");

                        _controller.ClipboardMonitor.Start(handle);
                        _clipDataService.NewClipAdded += OnNewClipAdded;
                        _controller.ProcessingFailed += OnProcessingFailed;
                        LogManager.LogDebug("Clipboard monitor started.");

                        _mainWindow.Width = settings.WindowWidth;
                        _mainWindow.Height = settings.WindowHeight;

                        if (_mainViewModel != null)
                        {
                            _mainViewModel.AlwaysOnTopChanged += OnViewModelAlwaysOnTopChanged;
                            _mainViewModel.IsReadyForEvents = true;
                        }

                        InitializeTrayIcon();
                        LogManager.LogDebug("Tray icon initialized.");

                        if (_mainViewModel != null)
                        {
                            _mainViewModel.IsInitializing = false;
                            _ = _mainViewModel.LoadClipsAsync();
                            LogManager.LogInfo("Initialization COMPLETE.");
                        }
                    }
                    catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException)
                    {
                        LogManager.LogCritical(ex, "FATAL: Unhandled exception during Dispatcher.InvokeAsync in ApplicationHostService.StartAsync.");
                        System.Windows.MessageBox.Show($"A critical error occurred during startup and has been logged. The application will now exit.\n\nError: {ex.Message}", "Cliptoo Startup Error", System.Windows.MessageBoxButton.OK, System.Windows.MessageBoxImage.Error);
                        Application.Current.Shutdown();
                    }
                });
            }
            catch (Exception ex) when (ex is IOException or SqliteException)
            {
                LogManager.LogCritical(ex, "FATAL: Unhandled exception in ApplicationHostService.StartAsync before dispatcher invocation.");
                System.Windows.MessageBox.Show($"A critical error occurred before the UI could be initialized. Please check the logs.\n\nError: {ex.Message}", "Cliptoo Startup Error", System.Windows.MessageBoxButton.OK, System.Windows.MessageBoxImage.Error);
                Application.Current.Shutdown();
            }
        }

        public Task StopAsync(CancellationToken cancellationToken)
        {
            if (_mainViewModel != null)
            {
                _mainViewModel.AlwaysOnTopChanged -= OnViewModelAlwaysOnTopChanged;
                _mainViewModel.Cleanup();
            }
            if (_notifyIconService.IsRegistered)
            {
                _notifyIconService.Unregister();
            }

            _settingsService.SettingsChanged -= OnSettingsChanged;
            if (_controller != null)
            {
                _controller.ProcessingFailed -= OnProcessingFailed;
            }
            if (_clipDataService != null)
            {
                _clipDataService.NewClipAdded -= OnNewClipAdded;
            }
            _controller?.Dispose();
            Dispose();

            return Task.CompletedTask;
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

            Application.Current.Dispatcher.Invoke(() =>
            {
                _themeService.ApplyThemeFromSettings();
            });
        }

        private void OnNewClipAdded(object? sender, EventArgs e)
        {
            _appInteractionService.NotifyUiActivity();
        }

        private void InitializeTrayIcon()
        {
            if (_mainWindow is null) return;

            _notifyIconService.SetParentWindow(_mainWindow);

            var iconUri = new Uri("pack://application:,,,/Assets/Icons/cliptoo.ico");
            var streamInfo = Application.GetResourceStream(iconUri);
            if (streamInfo != null)
            {
                using (var stream = streamInfo.Stream)
                {
                    var bitmap = new BitmapImage();
                    bitmap.BeginInit();
                    bitmap.StreamSource = stream;
                    bitmap.CacheOption = BitmapCacheOption.OnLoad;
                    bitmap.EndInit();
                    bitmap.Freeze();
                    _notifyIconService.Icon = bitmap;
                }
            }

            _notifyIconService.TooltipText = "Cliptoo";

            _alwaysOnTopMenuItem = new System.Windows.Controls.MenuItem { Header = "Always on Top", IsCheckable = true, Tag = AppConstants.UITags.AlwaysOnTop };
            _alwaysOnTopMenuItem.Click += OnMenuItemClick;

            _showHideMenuItem = new System.Windows.Controls.MenuItem { Header = "Show Cliptoo", Tag = AppConstants.UITags.ShowHide };
            _showHideMenuItem.Click += OnMenuItemClick;

            var settingsMenuItem = new System.Windows.Controls.MenuItem { Header = "Settings..." };
            settingsMenuItem.Click += (s, e) => _mainViewModel?.OpenSettingsCommand.Execute(null);

            var quitMenuItem = new System.Windows.Controls.MenuItem { Header = "Quit", Tag = AppConstants.UITags.Quit };
            quitMenuItem.Click += OnMenuItemClick;

            var contextMenu = new System.Windows.Controls.ContextMenu
            {
                Items =
            {
                _showHideMenuItem,
                _alwaysOnTopMenuItem,
                new Separator(),
                settingsMenuItem,
                new Separator(),
                quitMenuItem
            }
            };
            contextMenu.Opened += OnContextMenuOpened;

            _notifyIconService.ContextMenu = contextMenu;

            if (_notifyIconService is CustomNotifyIconService customNotifyIconService)
            {
                customNotifyIconService.LeftClicked += OnTrayLeftClicked;
            }

            _notifyIconService.Register();

            if (_mainViewModel != null) _mainViewModel.IsAlwaysOnTop = _settingsService.Settings.IsAlwaysOnTop;
            _alwaysOnTopMenuItem.IsChecked = _settingsService.Settings.IsAlwaysOnTop;
        }

        private IntPtr HwndHook(IntPtr hwnd, int msg, IntPtr wparam, IntPtr lparam, ref bool handled)
        {
            if (msg == _taskbarCreatedMessageId)
            {
                LogManager.LogInfo("Taskbar has been recreated (Explorer likely restarted). Re-registering tray icon.");
                _notifyIconService.Unregister();
                _notifyIconService.Register();
                handled = true;
                return IntPtr.Zero;
            }

            switch (msg)
            {
                case WM_CLIPBOARDUPDATE:
                    _controller.ClipboardMonitor.ProcessSystemUpdate();
                    handled = true;
                    break;
                case WM_HOTKEY:
                    _globalHotkey?.OnHotkeyPressed();
                    handled = true;
                    break;
                case WM_MOUSEACTIVATE:
                    if (_mainWindow != null && !_mainWindow.IsActive && _mainViewModel != null && _mainViewModel.IsAlwaysOnTop)
                    {
                        handled = true;
                        return (IntPtr)MA_NOACTIVATE;
                    }
                    break;
            }
            return IntPtr.Zero;
        }

        private void OnContextMenuOpened(object sender, RoutedEventArgs e)
        {
            if (_showHideMenuItem != null && _mainWindow != null)
            {
                _showHideMenuItem.Header = _mainWindow.IsVisible ? "Send to Tray" : "Show Cliptoo";
            }
        }

        private void OnViewModelAlwaysOnTopChanged(object? sender, BoolEventArgs e)
        {
            if (_alwaysOnTopMenuItem != null)
            {
                _alwaysOnTopMenuItem.IsChecked = e.Value;
            }
        }

        private void OnMenuItemClick(object sender, RoutedEventArgs e)
        {
            if (sender is not System.Windows.Controls.MenuItem menuItem || menuItem.Tag is not string tag) return;

            switch (tag)
            {
                case AppConstants.UITags.ShowHide:
                    ToggleWindowVisibility(true);
                    break;
                case AppConstants.UITags.AlwaysOnTop:
                    if (_mainViewModel != null)
                    {
                        _mainViewModel.IsAlwaysOnTop = !_mainViewModel.IsAlwaysOnTop;
                    }
                    break;
                case AppConstants.UITags.Quit:
                    Application.Current.Shutdown();
                    break;
            }
        }

        private void OnTrayLeftClicked() => ToggleWindowVisibility(true);

        private void OnHotkeyPressed(object? sender, EventArgs e) => ToggleWindowVisibility(false);

        private void ToggleWindowVisibility(bool isTrayRequest)
        {
            if ((DateTime.UtcNow - _lastToggleVisibility).TotalMilliseconds < 200)
            {
                return;
            }
            _lastToggleVisibility = DateTime.UtcNow;

            if (_mainWindow == null) return;

            if (_mainWindow.IsVisible)
            {
                if (_mainViewModel != null) _mainViewModel.IsHidingExplicitly = true;
                Application.Current.Dispatcher.BeginInvoke(new Action(() =>
                {
                    _mainViewModel?.HideWindow();
                    _appInteractionService.IsUiInteractive = false;
                }));
            }
            else
            {
                _appInteractionService.IsUiInteractive = true;
                _appInteractionService.NotifyUiActivity();
                _windowPositioner.PositionWindow(_mainWindow, _settingsService.Settings, isTrayRequest);
                _mainWindow.Show();
                _mainWindow.WindowState = WindowState.Normal;
                _mainWindow.Activate();
                _mainWindow.Focus();
            }
        }

        public void Dispose()
        {
            _globalHotkey?.Dispose();
            _hwndSource?.RemoveHook(HwndHook);
            _hwndSource?.Dispose();
            GC.SuppressFinalize(this);
        }

        private void OnProcessingFailed(object? sender, ProcessingFailedEventArgs e)
        {
            _notificationService.Show(e.Title, e.Message, ControlAppearance.Danger, SymbolRegular.ErrorCircle24, 5);
        }
    }
}