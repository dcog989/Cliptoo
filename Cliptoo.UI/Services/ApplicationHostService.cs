using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Native;
using Cliptoo.Core.Services.Models;
using Cliptoo.UI.ViewModels;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Wpf.Ui;
using Wpf.Ui.Appearance;
using Wpf.Ui.Controls;
using Wpf.Ui.Tray;

namespace Cliptoo.UI.Services
{
    public class ApplicationHostService : IHostedService, IDisposable
    {
        private const int WM_CLIPBOARDUPDATE = 0x031D;
        private const int WM_HOTKEY = 0x0312;
        private const int WM_MOUSEACTIVATE = 0x0021;
        private const int MA_NOACTIVATE = 3;
        private const double OKLCH_CHROMA_BRIGHT = 0.22;
        private const double OKLCH_CHROMA_MUTED = 0.10;
        private DateTime _lastToggleVisibility = DateTime.MinValue;

        private readonly INotificationService _notificationService;
        private readonly IServiceProvider _serviceProvider;
        private readonly INotifyIconService _notifyIconService;
        private readonly CliptooController _controller;
        private readonly IWindowPositioner _windowPositioner;
        private readonly ISnackbarService _snackbarService;
        private MainWindow? _mainWindow;
        private MainViewModel? _mainViewModel;
        private IGlobalHotkey? _globalHotkey;
        private HwndSource? _hwndSource;

        private System.Windows.Controls.MenuItem? _alwaysOnTopMenuItem;
        private System.Windows.Controls.MenuItem? _showHideMenuItem;
        private string? _currentHotkey;

        public ApplicationHostService(IServiceProvider serviceProvider, INotifyIconService notifyIconService, CliptooController controller, IWindowPositioner windowPositioner, ISnackbarService snackbarService, INotificationService notificationService)
        {
            _serviceProvider = serviceProvider;
            _notifyIconService = notifyIconService;
            _controller = controller;
            _windowPositioner = windowPositioner;
            _snackbarService = snackbarService;
            _notificationService = notificationService;
        }

        public async Task StartAsync(CancellationToken cancellationToken)
        {
            Cliptoo.Core.Configuration.LogManager.Log("DIAG: ApplicationHostService.StartAsync START");
            await _controller.InitializeAsync();

            await Application.Current.Dispatcher.InvokeAsync(async () =>
            {
                _controller.SettingsChanged += OnSettingsChanged;

                var settings = _controller.GetSettings();
                _currentHotkey = settings.Hotkey;

                _mainWindow = _serviceProvider.GetRequiredService<MainWindow>();
                _mainWindow.MaxHeight = SystemParameters.WorkArea.Height * 0.9;

                _mainViewModel = _serviceProvider.GetRequiredService<MainWindow>().DataContext as MainViewModel;
                if (_mainViewModel != null)
                {
                    Cliptoo.Core.Configuration.LogManager.Log("DIAG_LOAD: AHS - Initializing MainViewModel...");
                    await _mainViewModel.InitializeAsync();
                    _mainViewModel.IsAlwaysOnTop = settings.IsAlwaysOnTop;
                    Cliptoo.Core.Configuration.LogManager.Log("DIAG_LOAD: AHS - Calling InitializeFirstFilter...");
                    _mainViewModel.InitializeFirstFilter();
                    LogManager.Log("DIAG: ApplicationHostService - MainViewModel initialized.");
                }

                ApplyTheme(settings.Theme);
                ApplyAccentFromSettings(settings);

                // This Show/Hide sequence is necessary to create the window handle (HWND)
                // so that the tray icon, clipboard monitor, and hotkeys can be registered.
                _mainWindow.Opacity = 0;
                _mainWindow.WindowStartupLocation = WindowStartupLocation.Manual;
                _mainWindow.Left = -9999;
                Cliptoo.Core.Configuration.LogManager.Log("DIAG: ApplicationHostService - Calling _mainWindow.Show()");
                _mainWindow.Show();
                _snackbarService.SetSnackbarPresenter(_mainWindow.SnackbarPresenter);
                Cliptoo.Core.Configuration.LogManager.Log("DIAG: ApplicationHostService - Calling _mainWindow.Hide()");
                _mainWindow.Hide();
                _mainWindow.Opacity = 1;

                var windowInteropHelper = new WindowInteropHelper(_mainWindow);
                IntPtr handle = windowInteropHelper.EnsureHandle();

                _hwndSource = HwndSource.FromHwnd(handle);
                _hwndSource?.AddHook(HwndHook);

                _globalHotkey = new GlobalHotkey(handle);
                if (!_globalHotkey.Register(_currentHotkey))
                {
                    var message = $"Failed to register the global hotkey '{_currentHotkey}'. It may be in use by another application. You can set a new one in Settings via the tray icon.";
                    LogManager.Log(message);
                    System.Windows.MessageBox.Show(message, "Cliptoo Warning", System.Windows.MessageBoxButton.OK, MessageBoxImage.Warning);
                }
                else
                {
                    _globalHotkey.HotkeyPressed += OnHotkeyPressed;
                }

                _controller.ClipboardMonitor.Start(handle);
                _controller.ProcessingFailed += OnProcessingFailed;

                _mainWindow.Width = settings.WindowWidth;
                _mainWindow.Height = settings.WindowHeight;

                if (_mainViewModel != null)
                {
                    _mainViewModel.AlwaysOnTopChanged += OnViewModelAlwaysOnTopChanged;
                    Cliptoo.Core.Configuration.LogManager.Log("DIAG_LOAD: AHS - MainViewModel.IsReadyForEvents = true");
                    _mainViewModel.IsReadyForEvents = true;
                }

                InitializeTrayIcon();

                if (_mainViewModel != null)
                {
                    _mainViewModel.IsInitializing = false;
                    _ = _mainViewModel.LoadClipsAsync();
                    Cliptoo.Core.Configuration.LogManager.Log("DIAG_LOAD: AHS - Initializing COMPLETE.");
                }
            });
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

            _controller.SettingsChanged -= OnSettingsChanged;
            _controller.ProcessingFailed -= OnProcessingFailed;
            _controller.Dispose();
            Dispose();

            return Task.CompletedTask;
        }

        private void OnSettingsChanged(object? sender, EventArgs e)
        {
            var settings = _controller.GetSettings();
            if (_globalHotkey != null && _currentHotkey != settings.Hotkey)
            {
                _currentHotkey = settings.Hotkey;
                _globalHotkey.Register(_currentHotkey);
                LogManager.Log($"Global hotkey re-registered to: {_currentHotkey}");
            }

            Application.Current.Dispatcher.Invoke(() =>
            {
                ApplyTheme(settings.Theme);
                ApplyAccentFromSettings(settings);
            });
        }

        private void ApplyTheme(string themeName)
        {
            if (_mainWindow == null || !_mainWindow.IsLoaded)
            {
                return;
            }

            var windowHandle = new WindowInteropHelper(_mainWindow).Handle;
            if (windowHandle == IntPtr.Zero)
            {
                return;
            }

            SystemThemeWatcher.UnWatch(_mainWindow);

            var theme = themeName?.ToLowerInvariant() switch
            {
                "light" => ApplicationTheme.Light,
                "dark" => ApplicationTheme.Dark,
                _ => ApplicationTheme.Unknown,
            };

            if (theme == ApplicationTheme.Unknown)
            {
                SystemThemeWatcher.Watch(_mainWindow, WindowBackdropType.Mica, true);
            }
            else
            {
                ApplicationThemeManager.Apply(theme, WindowBackdropType.Mica, true);
            }
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

            if (_mainViewModel != null) _mainViewModel.IsAlwaysOnTop = _controller.GetSettings().IsAlwaysOnTop;
            if (_alwaysOnTopMenuItem != null) _alwaysOnTopMenuItem.IsChecked = _controller.GetSettings().IsAlwaysOnTop;
        }

        private IntPtr HwndHook(IntPtr hwnd, int msg, IntPtr wparam, IntPtr lparam, ref bool handled)
        {
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

        private void OnViewModelAlwaysOnTopChanged(object? sender, bool isChecked)
        {
            if (_alwaysOnTopMenuItem != null)
            {
                _alwaysOnTopMenuItem.IsChecked = isChecked;
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
                    _controller.IsUiInteractive = false;
                }));
            }
            else
            {
                _controller.IsUiInteractive = true;
                _controller.NotifyUiActivity();
                _windowPositioner.PositionWindow(_mainWindow, _controller.GetSettings(), isTrayRequest);
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

        private void ApplyAccentFromSettings(Settings settings)
        {
            try
            {
                var color = (Color)ColorConverter.ConvertFromString(settings.AccentColor);
                Core.Services.ColorParser.RgbToOklch(color.R, color.G, color.B, out _, out _, out var h);

                var currentTheme = ApplicationThemeManager.GetAppTheme();
                if (currentTheme == ApplicationTheme.Unknown)
                {
                    currentTheme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
                }

                var lightness = currentTheme == ApplicationTheme.Dark ? 0.62 : 0.70;
                var hoverLightness = currentTheme == ApplicationTheme.Dark ? 0.68 : 0.64;
                var chroma = settings.AccentChromaLevel == "vibrant" ? OKLCH_CHROMA_BRIGHT : OKLCH_CHROMA_MUTED;

                var (ar, ag, ab) = Core.Services.ColorParser.OklchToRgb(lightness, chroma, h);
                var accentColor = Color.FromRgb(ar, ag, ab);
                var accentBrush = new SolidColorBrush(accentColor);
                accentBrush.Freeze();

                var (hr, hg, hb) = Core.Services.ColorParser.OklchToRgb(hoverLightness, chroma, h);
                var hoverColor = Color.FromRgb(hr, hg, hb);
                var hoverBrush = new SolidColorBrush(hoverColor);
                hoverBrush.Freeze();

                Application.Current.Resources["AccentBrush"] = accentBrush;
                Application.Current.Resources["AccentBrushHover"] = hoverBrush;

                ApplicationAccentColorManager.Apply(accentColor);
            }
            catch (FormatException ex)
            {
                LogManager.Log(ex, $"Invalid accent color format in settings: {settings.AccentColor}");
            }
        }

        private void OnProcessingFailed(object? sender, ProcessingFailedEventArgs e)
        {
            _notificationService.Show(e.Title, e.Message, ControlAppearance.Danger, SymbolRegular.ErrorCircle24, 5);
        }

    }
}