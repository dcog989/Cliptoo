using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using Cliptoo.Core;
using Cliptoo.Core.Database;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services.Models;
using Cliptoo.UI.ViewModels;
using Cliptoo.UI.Views;
using Microsoft.Data.Sqlite;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Wpf.Ui;
using Wpf.Ui.Controls;

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
        private readonly CliptooController _controller;
        private readonly ISettingsService _settingsService;
        private readonly IAppInteractionService _appInteractionService;
        private readonly IClipDataService _clipDataService;
        private readonly IWindowPositioner _windowPositioner;
        private readonly IDbManager _dbManager;
        private readonly IThemeService _themeService;
        private readonly IClipDisplayService _clipDisplayService;
        private readonly IUpdateService _updateService;
        private readonly IPlatformService _platformService;
        private readonly ITrayManagerService _trayManagerService;
        private readonly IStartupManagerService _startupManagerService;
        private MainWindow? _mainWindow;
        private MainViewModel? _mainViewModel;
        private HwndSource? _hwndSource;
        private uint _taskbarCreatedMessageId;

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern uint RegisterWindowMessage(string lpString);

        public ApplicationHostService(
            IServiceProvider serviceProvider,
            CliptooController controller,
            IWindowPositioner windowPositioner,
            INotificationService notificationService,
            ISettingsService settingsService,
            IAppInteractionService appInteractionService,
            IClipDataService clipDataService,
            IDbManager dbManager,
            IThemeService themeService,
            IClipDisplayService clipDisplayService,
            IUpdateService updateService,
            IPlatformService platformService,
            ITrayManagerService trayManagerService,
            IStartupManagerService startupManagerService)
        {
            _serviceProvider = serviceProvider;
            _controller = controller;
            _windowPositioner = windowPositioner;
            _notificationService = notificationService;
            _settingsService = settingsService;
            _appInteractionService = appInteractionService;
            _clipDataService = clipDataService;
            _dbManager = dbManager;
            _themeService = themeService;
            _clipDisplayService = clipDisplayService;
            _updateService = updateService;
            _platformService = platformService;
            _trayManagerService = trayManagerService;
            _startupManagerService = startupManagerService;
        }

        public async Task StartAsync(CancellationToken cancellationToken)
        {
            var assembly = Assembly.GetExecutingAssembly();
            var fvi = FileVersionInfo.GetVersionInfo(assembly.Location);
            var fullVersion = fvi.ProductVersion ?? fvi.FileVersion ?? assembly.GetName().Version?.ToString(3) ?? "0.0.0";
            var appVersion = fullVersion.Split('+')[0];

            var settings = _settingsService.Settings;
            LogManager.Configure(settings.LoggingLevel, settings.LogRetentionDays);
            LogManager.LogInfo($"##########################################################");
            LogManager.LogInfo($"Cliptoo v{appVersion} starting up...");
            LogManager.LogInfo($"##########################################################");
            LogManager.LogDebug("ApplicationHostService starting...");

            // Self-heal the startup registry key.
            // If the setting is enabled, ensure the registry points to the CURRENT executable path.
            // This handles cases where the app was moved or updated to a new folder.
            if (settings.StartWithWindows)
            {
                LogManager.LogDebug("StartWithWindows is enabled. Enforcing registry key update.");
                _startupManagerService.SetStartup(true);
            }

            await _updateService.CheckForUpdatesAsync(cancellationToken);
            if (cancellationToken.IsCancellationRequested) return;

            try
            {
                await InitializeCoreServicesAsync();
                await PreloadCommonIconsAsync(cancellationToken);
                if (cancellationToken.IsCancellationRequested) return;
                await Application.Current.Dispatcher.InvokeAsync(async () =>
                {
                    try
                    {
                        var handle = InitializeMainWindowAndGetHandle();
                        await InitializeViewModelAsync();

                        _platformService.Initialize(handle);
                        _platformService.HotkeyPressed += OnHotkeyPressed;

                        _trayManagerService.Initialize();
                        _trayManagerService.ToggleVisibilityRequested += OnToggleVisibilityRequested;

                        FinalizeAndLoadData();
                    }
                    catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException)
                    {
                        LogManager.LogCritical(ex, "FATAL: Unhandled exception during UI thread initialization in ApplicationHostService.StartAsync.");
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
                _mainViewModel.Cleanup();
            }

            if (_platformService != null)
            {
                _platformService.HotkeyPressed -= OnHotkeyPressed;
            }

            if (_trayManagerService != null)
            {
                _trayManagerService.ToggleVisibilityRequested -= OnToggleVisibilityRequested;
            }

            _controller.ProcessingFailed -= OnProcessingFailed;
            _clipDataService.NewClipAdded -= OnNewClipAdded;
            _controller?.Dispose();
            Dispose();

            return Task.CompletedTask;
        }

        private void OnNewClipAdded(object? sender, ClipAddedEventArgs e)
        {
            _appInteractionService.NotifyUiActivity();
        }

        private IntPtr HwndHook(IntPtr hwnd, int msg, IntPtr wparam, IntPtr lparam, ref bool handled)
        {
            if (msg == _taskbarCreatedMessageId)
            {
                LogManager.LogInfo("Taskbar has been recreated (Explorer likely restarted). Re-registering tray icon.");
                _trayManagerService.OnTaskbarCreated();
                handled = true;
                return IntPtr.Zero;
            }

            switch (msg)
            {
                case WM_CLIPBOARDUPDATE:
                    _platformService.OnClipboardUpdate();
                    handled = true;
                    break;
                case WM_HOTKEY:
                    _platformService.OnHotkeyPressed();
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

        private void OnToggleVisibilityRequested(object? sender, bool isTrayRequest) => ToggleWindowVisibility(isTrayRequest);

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
                }));
            }
            else
            {
                if (_mainViewModel != null) _mainViewModel.ActivationSourceIsTray = isTrayRequest;
                _appInteractionService.NotifyUiActivity();
                _windowPositioner.PositionWindow(_mainWindow, _settingsService.Settings, isTrayRequest);
                _mainWindow.Show();
                _mainWindow.WindowState = WindowState.Normal;
                _mainWindow.Activate();
                _mainWindow.Focus();
            }
        }

        private async Task InitializeCoreServicesAsync()
        {
            await _dbManager.InitializeAsync().ConfigureAwait(false);
            LogManager.LogDebug("Database initialized successfully.");
            await _controller.InitializeAsync().ConfigureAwait(false);
        }

        private IntPtr InitializeMainWindowAndGetHandle()
        {
            _settingsService.SettingsChanged += (s, e) => Application.Current.Dispatcher.Invoke(() => _themeService.ApplyThemeFromSettings());

            _mainWindow = _serviceProvider.GetRequiredService<MainWindow>();
            _mainWindow.MaxHeight = SystemParameters.WorkArea.Height * 0.9;

            _mainWindow.Opacity = 0;
            _mainWindow.WindowStartupLocation = WindowStartupLocation.Manual;
            _mainWindow.Left = -9999;
            _mainWindow.Show();

            var snackbarPresenter = _mainWindow.FindName("RootSnackbarPresenter") as SnackbarPresenter;
            if (snackbarPresenter != null)
            {
                var snackbarService = _serviceProvider.GetRequiredService<ISnackbarService>();
                snackbarService.SetSnackbarPresenter(snackbarPresenter);
            }

            _mainWindow.Hide();
            _mainWindow.Opacity = 1;

            var windowInteropHelper = new WindowInteropHelper(_mainWindow);
            var handle = windowInteropHelper.EnsureHandle();

            _themeService.Initialize(_mainWindow);

            _taskbarCreatedMessageId = RegisterWindowMessage("TaskbarCreated");
            _hwndSource = HwndSource.FromHwnd(handle);
            _hwndSource?.AddHook(HwndHook);

            return handle;
        }

        private async Task InitializeViewModelAsync()
        {
            _mainViewModel = _serviceProvider.GetRequiredService<MainViewModel>();
            if (_mainViewModel != null)
            {
                await _mainViewModel.InitializeAsync();
            }
        }

        private void FinalizeAndLoadData()
        {
            if (_mainWindow == null || _mainViewModel == null) return;

            var settings = _settingsService.Settings;
            _mainWindow.Width = settings.WindowWidth;
            _mainWindow.Height = settings.WindowHeight;

            _clipDataService.NewClipAdded += OnNewClipAdded;
            _controller.ProcessingFailed += OnProcessingFailed;

            _mainViewModel.IsReadyForEvents = true;
            _mainViewModel.IsInitializing = false;
            _ = _clipDisplayService.LoadClipsAsync();
            LogManager.LogInfo("Initialization COMPLETE.");
        }

        public void Dispose()
        {
            _platformService?.Dispose();
            _hwndSource?.RemoveHook(HwndHook);
            _hwndSource?.Dispose();
            GC.SuppressFinalize(this);
        }

        private void OnProcessingFailed(object? sender, ProcessingFailedEventArgs e)
        {
            _notificationService.Show(e.Title, e.Message, ControlAppearance.Danger, SymbolRegular.ErrorCircle24, 5);
        }

        private async Task PreloadCommonIconsAsync(CancellationToken cancellationToken)
        {
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            var iconProvider = _serviceProvider.GetRequiredService<IIconProvider>();

            var commonIconKeys = new List<(string key, int size)>
            {
                // From UiSharedResources
                (AppConstants.IconKeyLogo, 24),
                (AppConstants.IconKeyList, 28),
                (AppConstants.IconKeyWasTrimmed, 20),
                (AppConstants.IconKeyMultiline, 20),
                (AppConstants.IconKeyFavorite, 20),
                (AppConstants.IconKeyFavorite, 16),
                (AppConstants.IconKeyError, 32),
                (AppConstants.IconKeyTrash, 16),

                // Filter icons from ClipDisplayService
                (AppConstants.FilterKeyAll, 20),
                (AppConstants.ClipTypeArchive, 20),
                (AppConstants.ClipTypeAudio, 20),
                (AppConstants.ClipTypeCodeSnippet, 20),
                (AppConstants.ClipTypeColor, 20),
                (AppConstants.ClipTypeDanger, 20),
                (AppConstants.ClipTypeDatabase, 20),
                (AppConstants.ClipTypeDev, 20),
                (AppConstants.ClipTypeDocument, 20),
                (AppConstants.ClipTypeFolder, 20),
                (AppConstants.ClipTypeFont, 20),
                (AppConstants.ClipTypeGeneric, 20),
                (AppConstants.ClipTypeImage, 20),
                (AppConstants.ClipTypeLink, 20),
                (AppConstants.ClipTypeSystem, 20),
                (AppConstants.ClipTypeText, 20),
                (AppConstants.ClipTypeFileText, 20),
                (AppConstants.ClipTypeRtf, 20),
                (AppConstants.ClipTypeVideo, 20),

                // Other common icons
                (AppConstants.ClipTypeFileLink, 20),

                // Quick Paste icons
                ("1", 32), ("2", 32), ("3", 32), ("4", 32), ("5", 32),
                ("6", 32), ("7", 32), ("8", 32), ("9", 32),
            };

            var tasks = new List<Task>();
            foreach (var (key, size) in commonIconKeys.Distinct())
            {
                if (cancellationToken.IsCancellationRequested) return;
                tasks.Add(iconProvider.GetIconAsync(key, size));
            }
            await Task.WhenAll(tasks);
            stopwatch.Stop();
            LogManager.LogDebug($"PERF_DIAG: Pre-loaded {tasks.Count} common icons in {stopwatch.ElapsedMilliseconds}ms.");
        }

    }
}