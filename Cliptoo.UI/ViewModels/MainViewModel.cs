using System.Runtime;
using System.Collections.ObjectModel;
using System.Windows;
using System.Windows.Media;
using System.Windows.Threading;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using Wpf.Ui.Appearance;

namespace Cliptoo.UI.ViewModels
{
    public record FilterOption(string Name, string Key, ImageSource? Icon);

    public class BoolEventArgs : EventArgs
    {
        public bool Value { get; }
        public BoolEventArgs(bool value) { Value = value; }
    }

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
        private readonly IPreviewManager _previewManager;
        private readonly IComparisonStateService _comparisonStateService;
        private readonly IClipDisplayService _clipDisplayService;
        private readonly IUiSharedResources _uiSharedResources;
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
        private int _selectedIndex;
        private double _verticalScrollOffset;
        private bool _isPasting;
        private bool _isReadyForEvents; // Start false, ApplicationHostService will set it to true.
        public bool IsReadyForEvents { get => _isReadyForEvents; set => _isReadyForEvents = value; }
        public event EventHandler<BoolEventArgs>? AlwaysOnTopChanged;
        public event EventHandler? ListScrolledToTopRequest;
        public IPreviewManager PreviewManager => _previewManager;
        public ObservableCollection<ClipViewModel> Clips => _clipDisplayService.Clips;
        public ObservableCollection<FilterOption> FilterOptions => _clipDisplayService.FilterOptions;
        public bool IsCompareToolAvailable => _comparisonStateService.IsCompareToolAvailable;
        public bool IsHidingExplicitly { get; set; }
        public bool IsInitializing { get; set; } = true;
        public Settings CurrentSettings { get => _currentSettings; private set => SetProperty(ref _currentSettings, value); }
        public FontFamily MainFont { get => _mainFont; private set => SetProperty(ref _mainFont, value); }
        public FontFamily PreviewFont { get => _previewFont; private set => SetProperty(ref _previewFont, value); }
        public IUiSharedResources SharedResources => _uiSharedResources;
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
                    AlwaysOnTopChanged?.Invoke(this, new BoolEventArgs(value));
                    CurrentSettings.IsAlwaysOnTop = value;
                    _settingsService.SaveSettings();
                }
            }
        }

        public bool DisplayLogo => CurrentSettings.DisplayLogo;

        public string SearchTerm
        {
            get => _clipDisplayService.SearchTerm;
            set
            {
                if (_clipDisplayService.SearchTerm != value)
                {
                    _clipDisplayService.SearchTerm = value;
                    _appInteractionService.NotifyUiActivity();
                    OnPropertyChanged();
                }
            }
        }

        public FilterOption SelectedFilter
        {
            get => _clipDisplayService.SelectedFilter;
            set
            {
                if (_clipDisplayService.SelectedFilter != value)
                {
                    _clipDisplayService.SelectedFilter = value;
                    _appInteractionService.NotifyUiActivity();
                    OnPropertyChanged();
                }
            }
        }

        public MainViewModel(
            IClipDataService clipDataService,
            IClipboardService clipboardService,
            ISettingsService settingsService,
            IDatabaseService databaseService,
            IAppInteractionService appInteractionService,
            IServiceProvider serviceProvider,
            IClipViewModelFactory clipViewModelFactory,
            IPastingService pastingService,
            IFontProvider fontProvider,
            INotificationService notificationService,
            IIconProvider iconProvider,
            IPreviewManager previewManager,
            IComparisonStateService comparisonStateService,
            IClipDisplayService clipDisplayService,
            IUiSharedResources uiSharedResources)
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
            _previewManager = previewManager;
            _comparisonStateService = comparisonStateService;
            _clipDisplayService = clipDisplayService;
            _uiSharedResources = uiSharedResources;

            _currentSettings = _settingsService.Settings;
            _currentSettings.PropertyChanged += CurrentSettings_PropertyChanged;
            _mainFont = _fontProvider.GetFont(CurrentSettings.FontFamily);
            _previewFont = _fontProvider.GetFont(CurrentSettings.PreviewFontFamily);
            TooltipMaxHeight = SystemParameters.WorkArea.Height * 0.9;

            SendToTargets = new ObservableCollection<SendToTarget>(CurrentSettings.SendToTargets);

            PasteClipCommand = new RelayCommand(async param => await ExecutePasteClip(param, forcePlainText: null));
            PasteClipAsPlainTextCommand = new RelayCommand(async param => await ExecutePasteClip(param, forcePlainText: true));
            TransformAndPasteCommand = new RelayCommand(async param => await ExecuteTransformAndPaste(param));
            OpenSettingsCommand = new RelayCommand(_ => OpenSettingsWindow());
            HideWindowCommand = new RelayCommand(_ => HideWindow());
            LoadMoreClipsCommand = new RelayCommand(async _ => await _clipDisplayService.LoadMoreClipsAsync());
            TogglePinCommand = new RelayCommand(async p => await ExecuteTogglePin(p));
            DeleteClipCommand = new RelayCommand(async p => await ExecuteDeleteClip(p));
            EditClipCommand = new RelayCommand(p => ExecuteEditClip(p));
            MoveToTopCommand = new RelayCommand(async p => await ExecuteMoveToTop(p));
            OpenCommand = new RelayCommand(async p => await ExecuteOpen(p));
            SelectForCompareLeftCommand = new RelayCommand(p => ExecuteSelectForCompareLeft(p));
            CompareWithSelectedRightCommand = new RelayCommand(async p => await ExecuteCompareWithSelectedRight(p));
            SendToCommand = new RelayCommand(async p => await ExecuteSendTo(p));

            _clipDataService.NewClipAdded += OnNewClipAdded;
            _databaseService.HistoryCleared += OnHistoryCleared;
            _settingsService.SettingsChanged += OnSettingsChanged;
            _databaseService.CachesCleared += OnCachesCleared;
            _comparisonStateService.ComparisonStateChanged += OnComparisonStateChanged;
            _clipDisplayService.ListScrolledToTopRequest += (s, e) => ListScrolledToTopRequest?.Invoke(s, e);

            _clearClipsTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(30) };
            _clearClipsTimer.Tick += OnClearClipsTimerElapsed;
        }

        private void OnComparisonStateChanged(object? sender, ComparisonStateChangedEventArgs e)
        {
            // This logic updates all visible clips to reflect the current comparison state.
            // It's robust against virtualization recycling items.
            foreach (var clip in Clips)
            {
                if (e.NewLeftClipId.HasValue && clip.Id == e.NewLeftClipId.Value)
                {
                    clip.CompareLeftHeader = "✓ Comparing with this";
                    clip.ShowCompareRightOption = false; // Cannot compare with self
                }
                else
                {
                    clip.CompareLeftHeader = "Compare Left";
                    // Show "Compare Right" if a left clip is selected
                    clip.ShowCompareRightOption = e.NewLeftClipId.HasValue;
                }
            }
        }

        private void CurrentSettings_PropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
        {
            switch (e.PropertyName)
            {
                case nameof(Settings.FontFamily):
                    MainFont = _fontProvider.GetFont(CurrentSettings.FontFamily);
                    break;
                case nameof(Settings.PreviewFontFamily):
                    PreviewFont = _fontProvider.GetFont(CurrentSettings.PreviewFontFamily);
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
            await _clipDisplayService.InitializeAsync();
            await SharedResources.InitializeAsync();
            OnPropertyChanged(nameof(SelectedFilter)); // Ensure UI reflects initial filter
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

        private void OnClearClipsTimerElapsed(object? sender, EventArgs e)
        {
            _clearClipsTimer.Stop();
            if (!IsWindowVisible && Clips.Count > 0)
            {
                LogManager.LogInfo("Delayed timer elapsed. Clearing clips collection to conserve memory.");
                _clipDisplayService.ClearClipsForHiding();
            }
        }

        public void HideWindow()
        {
            IsHidingExplicitly = true;

            IsFilterPopupOpen = false;

            // Start a timer to clear the collection later, instead of immediately.
            _clearClipsTimer.Start();

            _needsRefreshOnShow = true;

            var settings = _settingsService.Settings;
            if (!settings.RememberSearchInput)
            {
                SearchTerm = string.Empty;
            }
            if (!settings.RememberFilterSelection && FilterOptions.Any())
            {
                SelectedFilter = FilterOptions.First();
            }

            Application.Current.MainWindow?.Hide();

            GCSettings.LatencyMode = GCLatencyMode.SustainedLowLatency;
            LogManager.LogDebug("GC mode set to SustainedLowLatency.");
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
                _clipDisplayService.RefreshClipList();
                _needsRefreshOnShow = false;
            }
        }

        public void Cleanup()
        {
            _currentSettings.PropertyChanged -= CurrentSettings_PropertyChanged;
            _clipDataService.NewClipAdded -= OnNewClipAdded;
            _databaseService.HistoryCleared -= OnHistoryCleared;
            _settingsService.SettingsChanged -= OnSettingsChanged;
            _databaseService.CachesCleared -= OnCachesCleared;
            _clearClipsTimer.Tick -= OnClearClipsTimerElapsed;
            _comparisonStateService.ComparisonStateChanged -= OnComparisonStateChanged;

            if (_clipDisplayService is IDisposable disposable)
            {
                disposable.Dispose();
            }

            foreach (var clip in Clips)
            {
                clip.Dispose();
            }
            Clips.Clear();
        }

        private void OnNewClipAdded(object? sender, EventArgs e)
        {
            if (!IsWindowVisible)
            {
                _needsRefreshOnShow = true;
                return;
            }
            if (!IsReadyForEvents) return;
            _clipDisplayService.RefreshClipList();
        }
        private void OnHistoryCleared(object? sender, EventArgs e) => _clipDisplayService.RefreshClipList();
        private void OnCachesCleared(object? sender, EventArgs e) => _clipDisplayService.RefreshClipList();

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

        public void HandleWindowDeactivated()
        {
            Application.Current.Dispatcher.InvokeAsync(async () =>
            {
                await Task.Delay(50);

                if (IsFilterPopupOpen)
                {
                    IsFilterPopupOpen = false;
                }

                if (PreviewManager.IsPreviewOpen)
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
            });
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

        public void RequestShowPreview(ClipViewModel? clipVm)
        {
            _previewManager.RequestShowPreview(clipVm);
        }

        public void RequestHidePreview()
        {
            _previewManager.RequestHidePreview();
        }

        public void TogglePreviewForSelection(UIElement? placementTarget)
        {
            var listView = (Application.Current.MainWindow as Views.MainWindow)?.ClipListView;
            if (listView?.SelectedItem is not ClipViewModel selectedVm) return;
            _previewManager.TogglePreviewForSelection(selectedVm, placementTarget);
        }
    }
}