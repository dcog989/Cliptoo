using System.Windows;
using System.Windows.Controls;
using System.Windows.Media.Imaging;
using Cliptoo.Core;
using Cliptoo.Core.Interfaces;
using Cliptoo.UI.ViewModels;
using Cliptoo.UI.Views;
using Wpf.Ui.Tray;

namespace Cliptoo.UI.Services
{
    internal class TrayManagerService : ITrayManagerService
    {
        private readonly INotifyIconService _notifyIconService;
        private readonly MainViewModel _mainViewModel;
        private readonly ISettingsService _settingsService;
        private static MainWindow? MainWindow => Application.Current.MainWindow as MainWindow;

        private MenuItem? _alwaysOnTopMenuItem;
        private MenuItem? _showHideMenuItem;

        public event EventHandler<bool>? ToggleVisibilityRequested;

        public TrayManagerService(INotifyIconService notifyIconService, MainViewModel mainViewModel, ISettingsService settingsService)
        {
            _notifyIconService = notifyIconService;
            _mainViewModel = mainViewModel;
            _settingsService = settingsService;
        }

        public void Initialize()
        {
            if (MainWindow is null) return;

            _notifyIconService.SetParentWindow(MainWindow);

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

            _alwaysOnTopMenuItem = new MenuItem { Header = "Always on Top", IsCheckable = true, Tag = AppConstants.UITags.AlwaysOnTop };
            _alwaysOnTopMenuItem.Click += OnMenuItemClick;

            _showHideMenuItem = new MenuItem { Header = "Show Cliptoo", Tag = AppConstants.UITags.ShowHide };
            _showHideMenuItem.Click += OnMenuItemClick;

            var settingsMenuItem = new MenuItem { Header = "Settings..." };
            settingsMenuItem.Click += (s, e) => _mainViewModel.OpenSettingsCommand.Execute(null);

            var quitMenuItem = new MenuItem { Header = "Quit", Tag = AppConstants.UITags.Quit };
            quitMenuItem.Click += OnMenuItemClick;

            var contextMenu = new ContextMenu
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

            _mainViewModel.AlwaysOnTopChanged += OnViewModelAlwaysOnTopChanged;
            _mainViewModel.IsAlwaysOnTop = _settingsService.Settings.IsAlwaysOnTop;
            _alwaysOnTopMenuItem.IsChecked = _settingsService.Settings.IsAlwaysOnTop;
        }

        private void OnContextMenuOpened(object sender, RoutedEventArgs e)
        {
            if (_showHideMenuItem != null && MainWindow != null)
            {
                _showHideMenuItem.Header = MainWindow.IsVisible ? "Send to Tray" : "Show Cliptoo";
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
            if (sender is not MenuItem menuItem || menuItem.Tag is not string tag) return;

            switch (tag)
            {
                case AppConstants.UITags.ShowHide:
                    ToggleVisibilityRequested?.Invoke(this, true);
                    break;
                case AppConstants.UITags.AlwaysOnTop:
                    _mainViewModel.IsAlwaysOnTop = !_mainViewModel.IsAlwaysOnTop;
                    break;
                case AppConstants.UITags.Quit:
                    Application.Current.Shutdown();
                    break;
            }
        }

        private void OnTrayLeftClicked(object? sender, EventArgs e) => ToggleVisibilityRequested?.Invoke(this, true);

        public void OnTaskbarCreated()
        {
            _notifyIconService.Unregister();
            _notifyIconService.Register();
        }
    }
}