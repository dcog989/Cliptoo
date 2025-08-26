using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Native;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Wpf.Ui.Appearance;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    public record FilterOption(string Name, string Key, ImageSource? Icon);

    public class MainViewModel : ViewModelBase
    {
        private readonly CliptooController _controller;
        private readonly IServiceProvider _serviceProvider;
        private readonly IClipViewModelFactory _clipViewModelFactory;
        private readonly IPastingService _pastingService;
        private readonly IFontProvider _fontProvider;
        private readonly INotificationService _notificationService;
        private readonly IIconProvider _iconProvider;
        private uint _currentOffset;
        private bool _isLoadingMore;
        private const uint PageSize = 50;
        private CancellationTokenSource _loadClipsCts = new();
        private string _searchTerm = string.Empty;
        private FilterOption _selectedFilter;
        private readonly DispatcherTimer _debounceTimer;
        private bool _isAlwaysOnTop;
        private Settings _currentSettings;
        private bool _isFilterPopupOpen;
        private bool _isQuickPasteModeActive;
        private bool _isWindowVisible;
        private bool _needsRefreshOnShow = true;
        private bool _canLoadMore = true;
        public bool IsWindowVisible { get => _isWindowVisible; set => SetProperty(ref _isWindowVisible, value); }
        private FontFamily _mainFont;
        private FontFamily _previewFont;
        private int? _leftCompareClipId;
        private bool _isPasting;
        private readonly DispatcherTimer _showPreviewTimer;
        private readonly DispatcherTimer _hidePreviewTimer;
        private WeakReference<ClipViewModel>? _previewClipRef;
        private bool _isPreviewOpen;
        private bool _isReadyForEvents = false; // Start false, ApplicationHostService will set it to true.
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
        public event Action<bool>? AlwaysOnTopChanged;
        public event Action? ListScrolledToTopRequest;
        public ObservableCollection<ClipViewModel> Clips { get; }
        public ObservableCollection<FilterOption> FilterOptions { get; }
        public ICommand PasteClipCommand { get; }
        public ICommand OpenSettingsCommand { get; }
        public ICommand HideWindowCommand { get; }
        public ICommand LoadMoreClipsCommand { get; }
        public bool IsCompareToolAvailable { get; }
        public bool IsHidingExplicitly { get; set; }
        public bool IsInitializing { get; set; } = true;
        public ClipViewModel? PreviewClip => _previewClipRef != null && _previewClipRef.TryGetTarget(out var target) ? target : null;
        public bool IsPreviewOpen { get => _isPreviewOpen; set => SetProperty(ref _isPreviewOpen, value); }
        public Settings CurrentSettings { get => _currentSettings; private set => SetProperty(ref _currentSettings, value); }
        public FontFamily MainFont { get => _mainFont; private set => SetProperty(ref _mainFont, value); }
        public FontFamily PreviewFont { get => _previewFont; private set => SetProperty(ref _previewFont, value); }
        public ObservableCollection<SendToTarget> SendToTargets { get; }
        public double TooltipMaxHeight { get; }
        public string CurrentThemeString => Application.Current.Dispatcher.Invoke(() =>
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
            set => SetProperty(ref _isQuickPasteModeActive, value);
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
                    AlwaysOnTopChanged?.Invoke(value);
                    CurrentSettings.IsAlwaysOnTop = value;
                    _controller.SaveSettings(CurrentSettings);
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
                    _controller.NotifyUiActivity();
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
                Cliptoo.Core.Configuration.LogManager.Log($"DIAG_LOAD: SelectedFilter setter called with '{value?.Key}'. Current is '{_selectedFilter?.Key}'. IsInitializing: {IsInitializing}");
                if (_selectedFilter == value || value == null) return;

                if (SetProperty(ref _selectedFilter, value))
                {
                    _controller.NotifyUiActivity();
                    if (IsReadyForEvents && !IsInitializing)
                    {
                        _ = LoadClipsAsync();
                    }
                }
            }
        }

        public MainViewModel(CliptooController controller, IServiceProvider serviceProvider, IClipViewModelFactory clipViewModelFactory, IPastingService pastingService, IFontProvider fontProvider, INotificationService notificationService, IIconProvider iconProvider)
        {
            _controller = controller;
            _serviceProvider = serviceProvider;
            _clipViewModelFactory = clipViewModelFactory;
            _pastingService = pastingService;
            _fontProvider = fontProvider;
            _notificationService = notificationService;
            _iconProvider = iconProvider;
            _currentSettings = _controller.GetSettings();
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

            _controller.NewClipAdded += OnNewClipAdded;
            _controller.HistoryCleared += RefreshClipList;
            _controller.SettingsChanged += OnSettingsChanged;
            _controller.CachesCleared += RefreshClipList;

            _debounceTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(300) };
            _debounceTimer.Tick += OnDebounceTimerElapsed;

            _showPreviewTimer = new DispatcherTimer();
            _showPreviewTimer.Tick += OnShowPreviewTimerTick;
            _hidePreviewTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(100) };
            _hidePreviewTimer.Tick += OnHidePreviewTimerTick;

            IsCompareToolAvailable = _controller.IsCompareToolAvailable();
        }

        public async Task InitializeAsync()
        {
            await InitializeFilterOptionsAsync();
            await LoadStaticIconsAsync();
        }

        public void InitializeFirstFilter()
        {
            Cliptoo.Core.Configuration.LogManager.Log($"DIAG_LOAD: InitializeFirstFilter called.");
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

        private void OnSettingsChanged()
        {
            Application.Current.Dispatcher.Invoke(() =>
            {
                CurrentSettings = _controller.GetSettings();
                MainFont = _fontProvider.GetFont(CurrentSettings.FontFamily);
                PreviewFont = _fontProvider.GetFont(CurrentSettings.PreviewFontFamily);

                SendToTargets.Clear();
                foreach (var target in CurrentSettings.SendToTargets)
                {
                    SendToTargets.Add(target);
                }

                UpdateAllClipViewModelsAppearance();
                OnPropertyChanged(nameof(DisplayLogo));
                foreach (var clipVM in Clips)
                {
                    clipVM.RaisePasteAsPropertiesChanged();
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

        private async Task PerformPasteAction(ClipViewModel clipVM, Func<Clip, Task> pasteAction)
        {
            if (IsPasting) return;

            IsPasting = true;
            try
            {
                var clip = await _controller.GetClipByIdAsync(clipVM.Id);
                if (clip == null) return;

                var stopwatch = Stopwatch.StartNew();
                while ((KeyboardUtils.IsControlPressed() || KeyboardUtils.IsAltPressed()) && stopwatch.ElapsedMilliseconds < 500)
                {
                    await Task.Delay(20);
                }

                HideWindow();

                await pasteAction(clip);
                await _controller.UpdatePasteCountAsync();

                await LoadClipsAsync(true);
            }
            finally
            {
                IsPasting = false;
            }
        }

        private async Task ExecutePasteClip(object? parameter)
        {
            if (parameter is ClipViewModel clipVM)
            {
                await PerformPasteAction(clipVM, clip => _pastingService.PasteClipAsync(clip));
            }
        }

        private void OpenSettingsWindow()
        {
            var settingsWindow = _serviceProvider.GetRequiredService<SettingsWindow>();
            settingsWindow.Owner = Application.Current.MainWindow;
            settingsWindow.ShowDialog();
        }

        private void ShowClipEditor(ClipViewModel clipVM)
        {
            var wasAlwaysOnTop = this.IsAlwaysOnTop;
            if (wasAlwaysOnTop)
            {
                this.IsAlwaysOnTop = false;
            }

            var viewerViewModel = new ClipViewerViewModel(clipVM.Id, _controller, _serviceProvider.GetRequiredService<IFontProvider>());
            viewerViewModel.OnClipUpdated += RefreshClipList;

            var viewerWindow = _serviceProvider.GetRequiredService<Views.ClipViewerWindow>();
            viewerWindow.Owner = Application.Current.MainWindow;
            viewerWindow.DataContext = viewerViewModel;

            void OnViewerWindowClosed(object? s, EventArgs e)
            {
                viewerWindow.Closed -= OnViewerWindowClosed;
                if (wasAlwaysOnTop)
                {
                    this.IsAlwaysOnTop = true;
                }
            }

            viewerWindow.Closed += OnViewerWindowClosed;
            viewerWindow.Show();
        }

        public async Task LoadClipsAsync(bool scrollToTop = false)
        {
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            Cliptoo.Core.Configuration.LogManager.Log($"DIAG_LOAD: LoadClipsAsync called. IsReadyForEvents: {IsReadyForEvents}");
            if (!_isReadyForEvents)
            {
                return;
            }
            _loadClipsCts.Cancel();
            _loadClipsCts = new CancellationTokenSource();
            var token = _loadClipsCts.Token;

            _isLoadingMore = true;
            _canLoadMore = true;

            try
            {
                string localSearchTerm = SearchTerm;
                string localFilterKey = SelectedFilter?.Key ?? AppConstants.FilterKeys.All;

                if (string.IsNullOrEmpty(localFilterKey))
                {
                    localFilterKey = AppConstants.FilterKeys.All;
                }

                if (!string.IsNullOrEmpty(localSearchTerm) && localSearchTerm.Length < 2)
                {
                    _currentOffset = 0;
                    return;
                }

                _currentOffset = 0;

                var clipsData = await _controller.GetClipsAsync(limit: PageSize, offset: _currentOffset, searchTerm: localSearchTerm, filterType: localFilterKey, cancellationToken: token);

                if (clipsData.Count < PageSize)
                {
                    _canLoadMore = false;
                }

                if (token.IsCancellationRequested) return;

                await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    if (token.IsCancellationRequested) return;

                    SyncClipsCollection(clipsData);

                    if (scrollToTop)
                    {
                        ListScrolledToTopRequest?.Invoke();
                    }
                });

                _currentOffset = (uint)clipsData.Count;
            }
            catch (OperationCanceledException)
            {
                LogManager.LogDebug("Search operation was cancelled.");
            }
            finally
            {
                _isLoadingMore = false;
                stopwatch.Stop();
                LogManager.LogDebug($"PERF_DIAG: MainViewModel.LoadClipsAsync (UI update included) completed in {stopwatch.ElapsedMilliseconds}ms.");
            }
        }

        private void SyncClipsCollection(List<Clip> newClips)
        {
            var theme = CurrentThemeString;
            var newClipIds = newClips.Select(c => c.Id).ToHashSet();

            var vmsToRemove = Clips.Where(vm => !newClipIds.Contains(vm.Id)).ToList();
            foreach (var vm in vmsToRemove)
            {
                Clips.Remove(vm);
            }

            var vmMap = Clips.ToDictionary(vm => vm.Id);

            for (int i = 0; i < newClips.Count; i++)
            {
                var clip = newClips[i];
                if (vmMap.TryGetValue(clip.Id, out var existingVm))
                {
                    existingVm.UpdateClip(clip, theme);
                    int currentVmIndex = Clips.IndexOf(existingVm);
                    if (currentVmIndex != i)
                    {
                        Clips.Move(currentVmIndex, i);
                    }
                }
                else
                {
                    var newVm = _clipViewModelFactory.Create(clip, CurrentSettings, theme, this);
                    ApplyAppearanceToViewModel(newVm);
                    Clips.Insert(i, newVm);
                }
            }
        }

        public async Task LoadMoreClipsAsync()
        {
            if (_isLoadingMore || !_canLoadMore || IsInitializing) return;

            if (!string.IsNullOrEmpty(SearchTerm) && SearchTerm.Length < 2)
            {
                return;
            }

            var token = _loadClipsCts.Token;
            _isLoadingMore = true;
            try
            {
                string localSearchTerm = SearchTerm;
                string localFilterKey = SelectedFilter?.Key ?? AppConstants.FilterKeys.All;

                if (string.IsNullOrEmpty(localFilterKey))
                {
                    localFilterKey = AppConstants.FilterKeys.All;
                }

                var clipsData = await _controller.GetClipsAsync(limit: PageSize, offset: _currentOffset, searchTerm: localSearchTerm, filterType: localFilterKey, cancellationToken: token);

                if (token.IsCancellationRequested) return;

                if (clipsData.Count > 0)
                {
                    await Application.Current.Dispatcher.InvokeAsync(() =>
                    {
                        if (token.IsCancellationRequested) return;

                        var theme = CurrentThemeString;

                        foreach (var clipData in clipsData)
                        {
                            var newVM = _clipViewModelFactory.Create(clipData, CurrentSettings, theme, this);
                            ApplyAppearanceToViewModel(newVM);
                            Clips.Add(newVM);
                        }
                        _currentOffset += (uint)clipsData.Count;
                        if (clipsData.Count < PageSize)
                        {
                            _canLoadMore = false;
                        }
                    });
                }
                else
                {
                    _canLoadMore = false;
                }
            }
            catch (OperationCanceledException)
            {
                LogManager.LogDebug($"LoadMoreClipsAsync operation was explicitly cancelled.");
            }
            finally
            {
                _isLoadingMore = false;
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

        private void UpdateAllClipViewModelsAppearance()
        {
            foreach (var clipVM in Clips)
            {
                ApplyAppearanceToViewModel(clipVM);
            }
        }

        public void RefreshClipList()
        {
            Cliptoo.Core.Configuration.LogManager.Log($"DIAG_LOAD: RefreshClipList called. IsWindowVisible: {IsWindowVisible}, IsReady: {IsReadyForEvents}");
            if (!IsWindowVisible)
            {
                _needsRefreshOnShow = true;
                return;
            }
            if (!_isReadyForEvents) return;
            Application.Current.Dispatcher.InvokeAsync(async () =>
            {
                await LoadClipsAsync();
            });
        }

        private void OnNewClipAdded()
        {
            Cliptoo.Core.Configuration.LogManager.Log($"DIAG_LOAD: OnNewClipAdded called. IsWindowVisible: {IsWindowVisible}, IsReady: {IsReadyForEvents}");
            if (!IsWindowVisible)
            {
                _needsRefreshOnShow = true;
                return;
            }
            if (!_isReadyForEvents) return;
            Application.Current.Dispatcher.InvokeAsync(async () =>
            {
                await LoadClipsAsync(true);
            });
        }

        private async void OnDebounceTimerElapsed(object? sender, EventArgs e)
        {
            _debounceTimer.Stop();
            await LoadClipsAsync(true);
        }

        public void RequestShowPreview(ClipViewModel? clipVm)
        {
            _hidePreviewTimer.Stop();
            _showPreviewTimer.Stop();

            if (clipVm == null)
            {
                return;
            }

            if (IsPreviewOpen && (PreviewClip == null || PreviewClip.Id != clipVm.Id))
            {
                IsPreviewOpen = false;
            }

            if (PreviewClip?.Id == clipVm.Id && IsPreviewOpen)
            {
                return;
            }

            if (!CurrentSettings.ShowHoverPreview)
            {
                return;
            }

            _previewClipRef = new WeakReference<ClipViewModel>(clipVm);
            _showPreviewTimer.Interval = TimeSpan.FromMilliseconds(CurrentSettings.HoverPreviewDelay);
            _showPreviewTimer.Start();
        }

        public void RequestHidePreview()
        {
            _showPreviewTimer.Stop();
            _hidePreviewTimer.Start();
        }

        private async void OnShowPreviewTimerTick(object? sender, EventArgs e)
        {
            _showPreviewTimer.Stop();
            var currentPreviewClip = PreviewClip;

            if (currentPreviewClip == null)
            {
                if (IsPreviewOpen)
                {
                    IsPreviewOpen = false;
                }
                return;
            }
            DebugUtils.LogMemoryUsage($"Before Tooltip Load (Clip ID: {currentPreviewClip.Id})");

            var loadTasks = new List<Task>
            {
                currentPreviewClip.LoadTooltipContentAsync()
            };

            if (currentPreviewClip.IsImage)
            {
                loadTasks.Add(currentPreviewClip.LoadImagePreviewAsync(currentPreviewClip.HoverImagePreviewSize));
            }

            await Task.WhenAll(loadTasks);
            DebugUtils.LogMemoryUsage($"After Tooltip Load (Clip ID: {currentPreviewClip.Id})");

            if (PreviewClip?.Id == currentPreviewClip.Id)
            {
                OnPropertyChanged(nameof(PreviewClip));
                if (!IsPreviewOpen)
                {
                    IsPreviewOpen = true;
                }
            }
        }

        private void OnHidePreviewTimerTick(object? sender, EventArgs e)
        {
            _hidePreviewTimer.Stop();
            if (IsPreviewOpen)
            {
                IsPreviewOpen = false;
                PreviewClip?.ClearTooltipContent();
                _previewClipRef = null;
                OnPropertyChanged(nameof(PreviewClip));
            }
        }

        public void HideWindow()
        {
            IsHidingExplicitly = true;
            _debounceTimer.Stop(); // Prevent pending searches from firing after hide.

            IsFilterPopupOpen = false;

            Clips.Clear();
            _currentOffset = 0;
            _needsRefreshOnShow = true;

            var settings = _controller.GetSettings();
            if (!settings.RememberSearchInput)
            {
                _searchTerm = string.Empty;
                OnPropertyChanged(nameof(SearchTerm));
            }
            if (!settings.RememberFilterSelection)
            {
                _selectedFilter = FilterOptions.First();
                OnPropertyChanged(nameof(SelectedFilter));
            }

            Application.Current.MainWindow?.Hide();
        }

        public void HandleWindowShown()
        {
            if (IsInitializing)
            {
                Cliptoo.Core.Configuration.LogManager.Log($"DIAG_LOAD: HandleWindowShown called during initialization. Aborting load.");
                return;
            }
            Cliptoo.Core.Configuration.LogManager.Log($"DIAG_LOAD: HandleWindowShown called. NeedsRefresh: {_needsRefreshOnShow}, IsReady: {IsReadyForEvents}");
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
            await _controller.MoveClipToTopAsync(clipVM.Id);
            await LoadClipsAsync(true);
        }

        public void HandleClipSelectForCompare(ClipViewModel clipVM) => LeftCompareClipId = (LeftCompareClipId == clipVM.Id) ? null : clipVM.Id;

        public async void HandleClipCompare(ClipViewModel clipVM)
        {
            if (!LeftCompareClipId.HasValue) return;

            var result = await _controller.CompareClipsAsync(LeftCompareClipId.Value, clipVM.Id);
            if (!result.success)
            {
                _notificationService.Show("Compare Failed", result.message, ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
            LeftCompareClipId = null;
        }

        public void Cleanup()
        {
            _controller.NewClipAdded -= OnNewClipAdded;
            _controller.HistoryCleared -= RefreshClipList;
            _controller.SettingsChanged -= OnSettingsChanged;
            _controller.CachesCleared -= RefreshClipList;
            _debounceTimer.Tick -= OnDebounceTimerElapsed;
        }

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

        public void TogglePreviewForSelection()
        {
            var listView = (Application.Current.MainWindow as MainWindow)?.ClipListView;
            if (listView?.SelectedItem is not ClipViewModel selectedVm) return;

            if (IsPreviewOpen && PreviewClip?.Id == selectedVm.Id)
            {
                RequestHidePreview();
            }
            else
            {
                // Stop any pending timers and immediately show the preview
                _showPreviewTimer.Stop();
                _hidePreviewTimer.Stop();
                _previewClipRef = new WeakReference<ClipViewModel>(selectedVm);
                OnShowPreviewTimerTick(null, EventArgs.Empty);
            }
        }

    }
}