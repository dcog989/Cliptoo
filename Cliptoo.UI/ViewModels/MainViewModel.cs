using System.Runtime;
using System.Collections.ObjectModel;
using System.Windows;
using System.Windows.Media;
using System.Windows.Threading;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Interfaces;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using Wpf.Ui.Appearance;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    public record FilterOption(string Name, string Key, ImageSource? Icon);

    public partial class MainViewModel : ViewModelBase
    {
        private readonly IClipDataService _clipDataService;
        private readonly IClipboardService _clipboardService;
        private readonly ISettingsService _settingsService;
        private readonly IDatabaseService _databaseService;
        private readonly IAppInteractionService _appInteractionService;
        private readonly IServiceProvider _serviceProvider;
        private readonly IClipViewModelFactory _clipViewModelFactory;
        private readonly IPastingService _pastingService;
        private readonly IFontProvider _fontProvider;
        private readonly INotificationService _notificationService;
        private readonly IIconProvider _iconProvider;
        private string _searchTerm = string.Empty;
        private FilterOption _selectedFilter;
        private readonly DispatcherTimer _debounceTimer;
        private readonly DispatcherTimer _clearClipsTimer;
        private bool _isAlwaysOnTop;
        private Settings _currentSettings;
        private bool _isFilterPopupOpen;
        private bool _isQuickPasteModeActive;
        private bool _isWindowVisible;
        private bool _needsRefreshOnShow = true;
        public bool IsWindowVisible { get => _isWindowVisible; set => SetProperty(ref _isWindowVisible, value); }
        private FontFamily _mainFont;
        private FontFamily _previewFont;
        private int? _leftCompareClipId;
        private int _selectedIndex;
        private double _verticalScrollOffset;
        private bool _isPasting;
        private bool _isReadyForEvents; // Start false, ApplicationHostService will set it to true.
        public bool IsReadyForEvents { get => _isReadyForEvents; set => _isReadyForEvents = value; }
        private ImageSource? _logoIcon;
        public ImageSource? LogoIcon { get => _logoIcon; private set => SetProperty(ref _logoIcon, value); }
        private ImageSource? _menuIcon;
        public ImageSource? MenuIcon { get => _menuIcon; private set => SetProperty(ref _menuIcon, value); }
        private ImageSource? _wasTrimmedIcon;
        public ImageSource? WasTrimmedIcon { get => _wasTrimmedIcon; private set => SetProperty(ref _wasTrimmedIcon, value); }
        private ImageSource? _multiLineIcon;
        public ImageSource? MultiLineIcon { get => _multiLineIcon; private set => SetProperty(ref _multiLineIcon, value); }
        private ImageSource? _pinIcon;
        public ImageSource? PinIcon { get => _pinIcon; private set => SetProperty(ref _pinIcon, value); }
        private ImageSource? _pinIcon16;
        public ImageSource? PinIcon16 { get => _pinIcon16; private set => SetProperty(ref _pinIcon16, value); }
        private ImageSource? _errorIcon;
        public ImageSource? ErrorIcon { get => _errorIcon; private set => SetProperty(ref _errorIcon, value); }
        public event EventHandler<bool>? AlwaysOnTopChanged;
        public event EventHandler? ListScrolledToTopRequest;
        public ObservableCollection<ClipViewModel> Clips { get; }
        public ObservableCollection<FilterOption> FilterOptions { get; }
        public bool IsCompareToolAvailable { get; }
        public bool IsHidingExplicitly { get; set; }
        public bool IsInitializing { get; set; } = true;
        public Settings CurrentSettings { get => _currentSettings; private set => SetProperty(ref _currentSettings, value); }
        public FontFamily MainFont { get => _mainFont; private set => SetProperty(ref _mainFont, value); }
        public FontFamily PreviewFont { get => _previewFont; private set => SetProperty(ref _previewFont, value); }
        public ObservableCollection<SendToTarget> SendToTargets { get; }
        public double TooltipMaxHeight { get; }
        public static string CurrentThemeString => Application.Current.Dispatcher.Invoke(() =>
        {
            var theme = ApplicationThemeManager.GetAppTheme();
            if (theme == ApplicationTheme.Unknown)
            {
                var systemTheme = ApplicationThemeManager.GetSystemTheme();
                theme = systemTheme == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
            }
            return theme == ApplicationTheme.Dark ? "dark" : "light";
        });

        public bool IsPasting { get => _isPasting; private set => SetProperty(ref _isPasting, value); }

        public int? LeftCompareClipId
        {
            get => _leftCompareClipId;
            set
            {
                var oldValue = _leftCompareClipId;
                if (SetProperty(ref _leftCompareClipId, value))
                {
                    UpdateCompareState(oldValue, value);
                }
            }
        }

        public bool IsFilterPopupOpen
        {
            get => _isFilterPopupOpen;
            set
            {
                if (_isFilterPopupOpen != value)
                {
                    SetProperty(ref _isFilterPopupOpen, value);
                }
            }
        }

        public bool IsQuickPasteModeActive
        {
            get => _isQuickPasteModeActive;
            set
            {
                if (SetProperty(ref _isQuickPasteModeActive, value))
                {
                    UpdateQuickPasteIndices();
                }
            }
        }

        public bool IsAlwaysOnTop
        {
            get => _isAlwaysOnTop;
            set
            {
                if (SetProperty(ref _isAlwaysOnTop, value))
                {
                    if (Application.Current.MainWindow != null)
                    {
                        Application.Current.MainWindow.Topmost = value;
                    }
                    AlwaysOnTopChanged?.Invoke(this, value);
                    CurrentSettings.IsAlwaysOnTop = value;
                    _settingsService.SaveSettings();
                }
            }
        }

        public bool DisplayLogo => CurrentSettings.DisplayLogo;

        public string SearchTerm
        {
            get => _searchTerm;
            set
            {
                if (_searchTerm != value)
                {
                    SetProperty(ref _searchTerm, value);
                    _appInteractionService.NotifyUiActivity();
                    _debounceTimer.Stop();
                    _debounceTimer.Start();
                }
            }
        }

        public FilterOption SelectedFilter
        {
            get => _selectedFilter;
            set
            {
                if (_selectedFilter == value || value == null) return;

                if (SetProperty(ref _selectedFilter, value))
                {
                    _appInteractionService.NotifyUiActivity();
                    if (IsReadyForEvents && !IsInitializing)
                    {
                        _ = LoadClipsAsync();
                    }
                }
            }
        }

        public MainViewModel(IClipDataService clipDataService, IClipboardService clipboardService, ISettingsService settingsService, IDatabaseService databaseService, IAppInteractionService appInteractionService, IServiceProvider serviceProvider, IClipViewModelFactory clipViewModelFactory, IPastingService pastingService, IFontProvider fontProvider, INotificationService notificationService, IIconProvider iconProvider)
        {
            _clipDataService = clipDataService;
            _clipboardService = clipboardService;
            _settingsService = settingsService;
            _databaseService = databaseService;
            _appInteractionService = appInteractionService;
            _serviceProvider = serviceProvider;
            _clipViewModelFactory = clipViewModelFactory;
            _pastingService = pastingService;
            _fontProvider = fontProvider;
            _notificationService = notificationService;
            _iconProvider = iconProvider;
            _currentSettings = _settingsService.Settings;
            _currentSettings.PropertyChanged += CurrentSettings_PropertyChanged;
            _mainFont = _fontProvider.GetFont(CurrentSettings.FontFamily);
            _previewFont = _fontProvider.GetFont(CurrentSettings.PreviewFontFamily);
            TooltipMaxHeight = SystemParameters.WorkArea.Height * 0.9;

            Clips = new ObservableCollection<ClipViewModel>();
            FilterOptions = new ObservableCollection<FilterOption>();
            SendToTargets = new ObservableCollection<SendToTarget>(CurrentSettings.SendToTargets);
            _selectedFilter = new FilterOption("All", AppConstants.FilterKeys.All, null);

            PasteClipCommand = new RelayCommand(async param => await ExecutePasteClip(param));
            OpenSettingsCommand = new RelayCommand(_ => OpenSettingsWindow());
            HideWindowCommand = new RelayCommand(_ => HideWindow());
            LoadMoreClipsCommand = new RelayCommand(async _ => await LoadMoreClipsAsync());

            _clipDataService.NewClipAdded += OnNewClipAdded;
            _databaseService.HistoryCleared += OnHistoryCleared;
            _settingsService.SettingsChanged += OnSettingsChanged;
            _databaseService.CachesCleared += OnCachesCleared;

            _debounceTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(300) };
            _debounceTimer.Tick += OnDebounceTimerElapsed;

            _clearClipsTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(30) };
            _clearClipsTimer.Tick += OnClearClipsTimerElapsed;

            _showPreviewTimer = new DispatcherTimer();
            _showPreviewTimer.Tick += OnShowPreviewTimerTick;
            _hidePreviewTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(100) };
            _hidePreviewTimer.Tick += OnHidePreviewTimerTick;

            IsCompareToolAvailable = _clipboardService.IsCompareToolAvailable();
        }

        private void CurrentSettings_PropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
        {
            switch (e.PropertyName)
            {
                case nameof(Settings.FontFamily):
                    MainFont = _fontProvider.GetFont(CurrentSettings.FontFamily);
                    foreach (var clipVM in Clips) clipVM.CurrentFontFamily = MainFont;
                    break;
                case nameof(Settings.FontSize):
                    foreach (var clipVM in Clips) clipVM.CurrentFontSize = CurrentSettings.FontSize;
                    break;
                case nameof(Settings.ClipItemPadding):
                    foreach (var clipVM in Clips) clipVM.PaddingSize = CurrentSettings.ClipItemPadding;
                    break;
                case nameof(Settings.PreviewFontFamily):
                    PreviewFont = _fontProvider.GetFont(CurrentSettings.PreviewFontFamily);
                    foreach (var clipVM in Clips) clipVM.PreviewFont = PreviewFont;
                    break;
                case nameof(Settings.PreviewFontSize):
                    foreach (var clipVM in Clips) clipVM.PreviewFontSize = CurrentSettings.PreviewFontSize;
                    break;
                case nameof(Settings.HoverImagePreviewSize):
                    foreach (var clipVM in Clips) clipVM.HoverImagePreviewSize = CurrentSettings.HoverImagePreviewSize;
                    break;
                case nameof(Settings.DisplayLogo):
                    OnPropertyChanged(nameof(DisplayLogo));
                    break;
                case nameof(Settings.PasteAsPlainText):
                    foreach (var clipVM in Clips) clipVM.NotifyPasteAsPropertiesChanged();
                    break;
            }
        }

        public async Task InitializeAsync()
        {
            await InitializeFilterOptionsAsync();
            await LoadStaticIconsAsync();
        }

        public void InitializeFirstFilter()
        {
            LogManager.LogDebug($"InitializeFirstFilter called.");
            _selectedFilter = FilterOptions.FirstOrDefault() ?? new FilterOption("All", AppConstants.FilterKeys.All, null);
            OnPropertyChanged(nameof(SelectedFilter));
        }

        private async Task LoadStaticIconsAsync()
        {
            LogoIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Logo, 24);
            MenuIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.List, 28);
            WasTrimmedIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.WasTrimmed, 20);
            MultiLineIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Multiline, 20);
            PinIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Pin, 20);
            PinIcon16 = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Pin, 16);
            ErrorIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Error, 32);
        }

        private async void OnSettingsChanged(object? sender, EventArgs e)
        {
            await Application.Current.Dispatcher.InvokeAsync(() =>
            {
                SendToTargets.Clear();
                foreach (var target in CurrentSettings.SendToTargets)
                {
                    SendToTargets.Add(target);
                }
            });
        }

        private void UpdateCompareState(int? oldClipId, int? newClipId)
        {
            if (oldClipId.HasValue)
            {
                var oldVm = Clips.FirstOrDefault(vm => vm.Id == oldClipId.Value);
                if (oldVm != null)
                {
                    oldVm.ShowCompareRightOption = newClipId.HasValue && oldVm.Id != newClipId.Value;
                    oldVm.CompareLeftHeader = "Compare Left";
                }
            }

            if (newClipId.HasValue)
            {
                var newVm = Clips.FirstOrDefault(vm => vm.Id == newClipId.Value);
                if (newVm != null)
                {
                    newVm.CompareLeftHeader = "✓ Comparing with this";
                }
            }

            // Update all other clips to show/hide the "Compare Right" option
            foreach (var vm in Clips.Where(c => c.Id != oldClipId && c.Id != newClipId))
            {
                vm.ShowCompareRightOption = newClipId.HasValue && vm.Id != newClipId.Value;
            }
        }

        private async Task InitializeFilterOptionsAsync()
        {
            var filterDisplayNames = new Dictionary<string, string>
            {
                { AppConstants.FilterKeys.All, "All" },
                { AppConstants.FilterKeys.Pinned, "Pinned" },
                { AppConstants.ClipTypes.Archive, "Archives" },
                { AppConstants.ClipTypes.Audio, "Audio" },
                { AppConstants.ClipTypes.Dev, "Dev Files" },
                { AppConstants.ClipTypes.CodeSnippet, "Code Snippets" },
                { AppConstants.ClipTypes.Color, "Colors" },
                { AppConstants.ClipTypes.Danger, "Dangerous" },
                { AppConstants.ClipTypes.Database, "Database Files" },
                { AppConstants.ClipTypes.Document, "Documents" },
                { AppConstants.ClipTypes.FileText, "Text Files" },
                { AppConstants.ClipTypes.Folder, "Folders" },
                { AppConstants.ClipTypes.Font, "Font Files" },
                { AppConstants.ClipTypes.Generic, "Generic Files" },
                { AppConstants.ClipTypes.Image, "Images" },
                { AppConstants.ClipTypes.Link, "Links" },
                { AppConstants.ClipTypes.Rtf, "Formatted Text" },
                { AppConstants.ClipTypes.System, "System Files" },
                { AppConstants.ClipTypes.Text, "Text" },
                { AppConstants.ClipTypes.Video, "Video" },
            };

            var orderedFilterKeys = new[]
            {
                AppConstants.FilterKeys.All,
                AppConstants.FilterKeys.Pinned,
                AppConstants.ClipTypes.Archive,
                AppConstants.ClipTypes.Audio,
                AppConstants.ClipTypes.CodeSnippet,
                AppConstants.ClipTypes.Color,
                AppConstants.ClipTypes.Danger,
                AppConstants.ClipTypes.Database,
                AppConstants.ClipTypes.Dev,
                AppConstants.ClipTypes.Document,
                AppConstants.ClipTypes.Folder,
                AppConstants.ClipTypes.Font,
                AppConstants.ClipTypes.Generic,
                AppConstants.ClipTypes.Image,
                AppConstants.ClipTypes.Link,
                AppConstants.ClipTypes.System,
                AppConstants.ClipTypes.Text,
                AppConstants.ClipTypes.FileText,
                AppConstants.ClipTypes.Rtf,
                AppConstants.ClipTypes.Video,
            };

            FilterOptions.Clear();
            foreach (var key in orderedFilterKeys)
            {
                var icon = await _iconProvider.GetIconAsync(key, 20);
                FilterOptions.Add(new FilterOption(filterDisplayNames[key], key, icon));
            }
        }

        private void ApplyAppearanceToViewModel(ClipViewModel clipVM)
        {
            clipVM.CurrentFontFamily = MainFont;
            clipVM.CurrentFontSize = CurrentSettings.FontSize;
            clipVM.PaddingSize = CurrentSettings.ClipItemPadding;
            clipVM.PreviewFont = PreviewFont;
            clipVM.PreviewFontSize = CurrentSettings.PreviewFontSize;
            clipVM.HoverImagePreviewSize = CurrentSettings.HoverImagePreviewSize;
        }

        private async void OnDebounceTimerElapsed(object? sender, EventArgs e)
        {
            _debounceTimer.Stop();
            await LoadClipsAsync(true);
        }

        private void OnClearClipsTimerElapsed(object? sender, EventArgs e)
        {
            _clearClipsTimer.Stop();
            if (!IsWindowVisible && Clips.Count > 0)
            {
                LogManager.Log("Delayed timer elapsed. Clearing clips collection to conserve memory.");
                Clips.Clear();
                _currentOffset = 0;
            }
        }

        public void HideWindow()
        {
            IsHidingExplicitly = true;
            _debounceTimer.Stop(); // Prevent pending searches from firing after hide.

            IsFilterPopupOpen = false;

            // Start a timer to clear the collection later, instead of immediately.
            _clearClipsTimer.Start();

            _needsRefreshOnShow = true;

            var settings = _settingsService.Settings;
            if (!settings.RememberSearchInput)
            {
                _searchTerm = string.Empty;
                OnPropertyChanged(nameof(SearchTerm));
            }
            if (!settings.RememberFilterSelection)
            {
                if (FilterOptions.Any())
                {
                    _selectedFilter = FilterOptions.First();
                    OnPropertyChanged(nameof(SelectedFilter));
                }
            }

            Application.Current.MainWindow?.Hide();

            // Hint to the GC to be more aggressive now that the UI is hidden.
            GCSettings.LargeObjectHeapCompactionMode = GCLargeObjectHeapCompactionMode.CompactOnce;
            GC.Collect(GC.MaxGeneration, GCCollectionMode.Forced, true, true);
            GCSettings.LatencyMode = GCLatencyMode.Batch;
            LogManager.LogDebug("GC mode set to Batch and collection requested.");
        }

        public void HandleWindowShown()
        {
            // Restore interactive GC mode for UI responsiveness.
            GCSettings.LatencyMode = GCLatencyMode.Interactive;
            LogManager.LogDebug("GC mode restored to Interactive.");

            // Stop the timer to prevent the collection from being cleared.
            _clearClipsTimer.Stop();

            if (IsInitializing)
            {
                return;
            }

            // If the clips were cleared by the timer, we must reload.
            if (Clips.Count == 0)
            {
                _needsRefreshOnShow = true;
            }

            if (_needsRefreshOnShow && IsReadyForEvents)
            {
                Application.Current.Dispatcher.InvokeAsync(async () =>
                {
                    await LoadClipsAsync(true);
                });
                _needsRefreshOnShow = false;
            }
        }

        public void HandleClipDeletion(ClipViewModel clipVM)
        {
            Application.Current.Dispatcher.Invoke(() => Clips.Remove(clipVM));
        }

        public void HandleClipEdit(ClipViewModel clipVM) => ShowClipEditor(clipVM);

        public void HandleClipPinToggle(ClipViewModel clipVM)
        {
            if (SelectedFilter.Key == AppConstants.FilterKeys.Pinned && !clipVM.IsPinned)
            {
                Application.Current.Dispatcher.Invoke(() => Clips.Remove(clipVM));
            }
        }

        public async Task HandleClipMoveToTop(ClipViewModel clipVM)
        {
            await _clipDataService.MoveClipToTopAsync(clipVM.Id);
            await LoadClipsAsync(true);
        }

        public void HandleClipSelectForCompare(ClipViewModel clipVM) => LeftCompareClipId = (LeftCompareClipId == clipVM.Id) ? null : clipVM.Id;

        public async void HandleClipCompare(ClipViewModel clipVM)
        {
            if (!LeftCompareClipId.HasValue) return;

            var result = await _clipboardService.CompareClipsAsync(LeftCompareClipId.Value, clipVM.Id);
            if (!result.success)
            {
                _notificationService.Show("Compare Failed", result.message, ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
            LeftCompareClipId = null;
        }

        public void Cleanup()
        {
            _currentSettings.PropertyChanged -= CurrentSettings_PropertyChanged;
            _clipDataService.NewClipAdded -= OnNewClipAdded;
            _databaseService.HistoryCleared -= OnHistoryCleared;
            _settingsService.SettingsChanged -= OnSettingsChanged;
            _databaseService.CachesCleared -= OnCachesCleared;
            _debounceTimer.Tick -= OnDebounceTimerElapsed;
            _clearClipsTimer.Tick -= OnClearClipsTimerElapsed;
        }

        private void OnNewClipAdded(object? sender, EventArgs e) => OnNewClipAdded();
        private void OnHistoryCleared(object? sender, EventArgs e) => RefreshClipList();
        private void OnCachesCleared(object? sender, EventArgs e) => RefreshClipList();

        private static T? FindVisualChild<T>(DependencyObject? obj) where T : DependencyObject
        {
            if (obj == null) return null;
            for (int i = 0; i < System.Windows.Media.VisualTreeHelper.GetChildrenCount(obj); i++)
            {
                DependencyObject child = System.Windows.Media.VisualTreeHelper.GetChild(obj, i);
                if (child is T dependencyObject)
                    return dependencyObject;
                T? childOfChild = FindVisualChild<T>(child);
                if (childOfChild != null)
                    return childOfChild;
            }
            return null;
        }

        public int SelectedIndex
        {
            get => _selectedIndex;
            set => SetProperty(ref _selectedIndex, value);
        }

        public double VerticalScrollOffset
        {
            get => _verticalScrollOffset;
            set
            {
                if (SetProperty(ref _verticalScrollOffset, value) && IsQuickPasteModeActive)
                {
                    UpdateQuickPasteIndices();
                }
            }
        }

        public async Task HandleWindowDeactivated()
        {
            await Task.Delay(50);

            if (IsFilterPopupOpen)
            {
                IsFilterPopupOpen = false;
            }

            if (IsPreviewOpen)
            {
                RequestHidePreview();
            }

            if (IsHidingExplicitly)
            {
                return;
            }

            if (Application.Current.Windows.OfType<Window>().Any(x => x != Application.Current.MainWindow && x.IsActive))
            {
                return;
            }

            if (Application.Current.MainWindow is not { IsVisible: true })
            {
                return;
            }

            if (!IsAlwaysOnTop)
            {
                HideWindow();
            }
        }

        public void UpdateQuickPasteIndices()
        {
            // Clear previous indices
            foreach (var clip in Clips.Where(c => c.Index > 0))
            {
                clip.Index = 0;
            }

            if (!IsQuickPasteModeActive) return;

            var firstVisibleIndex = (int)VerticalScrollOffset;

            for (var i = 0; i < 9; i++)
            {
                var targetIndex = firstVisibleIndex + i;
                if (targetIndex < Clips.Count)
                {
                    Clips[targetIndex].Index = i + 1;
                }
            }
        }

    }
}