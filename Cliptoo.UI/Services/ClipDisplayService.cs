using System.Collections.ObjectModel;
using System.Windows;
using System.Windows.Threading;
using Cliptoo.Core;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Services
{
    public class ClipDisplayService : IClipDisplayService, IDisposable
    {
        private readonly IClipDataService _clipDataService;
        private readonly IClipViewModelFactory _clipViewModelFactory;
        private readonly IIconProvider _iconProvider;

        private string _searchTerm = string.Empty;
        private FilterOption _selectedFilter;
        private readonly DispatcherTimer _debounceTimer;

        private uint _currentOffset;
        private bool _isLoadingMore;
        private const uint PageSize = 50;
        private CancellationTokenSource _loadClipsCts = new();
        private bool _canLoadMore = true;
        private bool _disposedValue;

        public ObservableCollection<ClipViewModel> Clips { get; } = new();
        public ObservableCollection<FilterOption> FilterOptions { get; } = new();

        public bool IsLoading => _isLoadingMore;
        public event EventHandler? ListScrolledToTopRequest;

        public ClipDisplayService(
            IClipDataService clipDataService,
            IClipViewModelFactory clipViewModelFactory,
            ISettingsService settingsService,
            IIconProvider iconProvider)
        {
            _clipDataService = clipDataService;
            _clipViewModelFactory = clipViewModelFactory;
            _iconProvider = iconProvider;

            _selectedFilter = new FilterOption("All", AppConstants.FilterKeys.All, null);

            _debounceTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(300) };
            _debounceTimer.Tick += OnDebounceTimerElapsed;

            Clips.CollectionChanged += (s, e) =>
            {
                if (e.OldItems != null)
                {
                    foreach (var item in e.OldItems)
                    {
                        if (item is IDisposable disposable) disposable.Dispose();
                    }
                }
            };
        }

        public string SearchTerm
        {
            get => _searchTerm;
            set
            {
                if (_searchTerm == value) return;
                _searchTerm = value;
                _debounceTimer.Stop();
                _debounceTimer.Start();
            }
        }

        public FilterOption SelectedFilter
        {
            get => _selectedFilter;
            set
            {
                if (_selectedFilter == value || value == null) return;
                _selectedFilter = value;
                _ = LoadClipsAsync(true);
            }
        }

        public async Task InitializeAsync()
        {
            await InitializeFilterOptionsAsync();
            SelectedFilter = FilterOptions.FirstOrDefault() ?? new FilterOption("All", AppConstants.FilterKeys.All, null);
        }

        private async void OnDebounceTimerElapsed(object? sender, EventArgs e)
        {
            _debounceTimer.Stop();
            await LoadClipsAsync(true);
        }

        public async Task LoadClipsAsync(bool scrollToTop = false)
        {
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            await _loadClipsCts.CancelAsync();
            _loadClipsCts.Dispose();
            _loadClipsCts = new CancellationTokenSource();
            var token = _loadClipsCts.Token;

            _isLoadingMore = true;
            _canLoadMore = true;

            try
            {
                string localSearchTerm = SearchTerm;
                string localFilterKey = SelectedFilter?.Key ?? AppConstants.FilterKeys.All;

                if (string.IsNullOrEmpty(localFilterKey)) localFilterKey = AppConstants.FilterKeys.All;
                if (!string.IsNullOrEmpty(localSearchTerm) && localSearchTerm.Length < 2)
                {
                    _currentOffset = 0;
                    return;
                }
                _currentOffset = 0;

                var clipsData = await _clipDataService.GetClipsAsync(PageSize, _currentOffset, localSearchTerm, localFilterKey, token);
                if (clipsData.Count < PageSize) _canLoadMore = false;
                if (token.IsCancellationRequested) return;

                await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    if (token.IsCancellationRequested) return;
                    Clips.Clear();
                    var theme = MainViewModel.CurrentThemeString;
                    foreach (var clip in clipsData)
                    {
                        Clips.Add(_clipViewModelFactory.Create(clip, theme));
                    }
                    if (scrollToTop) ListScrolledToTopRequest?.Invoke(this, EventArgs.Empty);
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
                LogManager.LogDebug($"PERF_DIAG: ClipDisplayService.LoadClipsAsync completed in {stopwatch.ElapsedMilliseconds}ms.");
            }
        }

        public async Task LoadMoreClipsAsync()
        {
            if (_isLoadingMore || !_canLoadMore) return;
            if (!string.IsNullOrEmpty(SearchTerm) && SearchTerm.Length < 2) return;

            var token = _loadClipsCts.Token;
            _isLoadingMore = true;
            try
            {
                var clipsData = await _clipDataService.GetClipsAsync(PageSize, _currentOffset, SearchTerm, SelectedFilter.Key, token);
                if (token.IsCancellationRequested) return;

                if (clipsData.Count > 0)
                {
                    await Application.Current.Dispatcher.InvokeAsync(() =>
                    {
                        if (token.IsCancellationRequested) return;
                        var theme = MainViewModel.CurrentThemeString;
                        foreach (var clipData in clipsData)
                        {
                            Clips.Add(_clipViewModelFactory.Create(clipData, theme));
                        }
                        _currentOffset += (uint)clipsData.Count;
                        if (clipsData.Count < PageSize) _canLoadMore = false;
                    });
                }
                else
                {
                    _canLoadMore = false;
                }
            }
            catch (OperationCanceledException)
            {
                LogManager.LogDebug("LoadMoreClipsAsync operation was cancelled.");
            }
            finally
            {
                _isLoadingMore = false;
            }
        }

        public void RefreshClipList()
        {
            Application.Current.Dispatcher.InvokeAsync(async () => await LoadClipsAsync(true));
        }

        public void ClearClipsForHiding()
        {
            Clips.Clear();
            _currentOffset = 0;
        }

        private async Task InitializeFilterOptionsAsync()
        {
            var filterDisplayNames = new Dictionary<string, string>
            {
                { AppConstants.FilterKeys.All, "All" }, { AppConstants.FilterKeys.Favorite, "Favorites" },
                { AppConstants.ClipTypes.Archive, "Archives" }, { AppConstants.ClipTypes.Audio, "Audio" },
                { AppConstants.ClipTypes.Dev, "Dev Files" }, { AppConstants.ClipTypes.CodeSnippet, "Code Snippets" },
                { AppConstants.ClipTypes.Color, "Colors" }, { AppConstants.ClipTypes.Danger, "Dangerous" },
                { AppConstants.ClipTypes.Database, "Database Files" }, { AppConstants.ClipTypes.Document, "Documents" },
                { AppConstants.ClipTypes.FileText, "Text Files" }, { AppConstants.ClipTypes.Folder, "Folders" },
                { AppConstants.ClipTypes.Font, "Font Files" }, { AppConstants.ClipTypes.Generic, "Generic Files" },
                { AppConstants.ClipTypes.Image, "Images" }, { AppConstants.ClipTypes.Link, "Links" },
                { AppConstants.ClipTypes.Rtf, "Formatted Text" }, { AppConstants.ClipTypes.System, "System Files" },
                { AppConstants.ClipTypes.Text, "Text" }, { AppConstants.ClipTypes.Video, "Video" },
            };
            var orderedFilterKeys = new[]
            {
                AppConstants.FilterKeys.All, AppConstants.FilterKeys.Favorite, AppConstants.ClipTypes.Archive, AppConstants.ClipTypes.Audio,
                AppConstants.ClipTypes.CodeSnippet, AppConstants.ClipTypes.Color, AppConstants.ClipTypes.Danger, AppConstants.ClipTypes.Database,
                AppConstants.ClipTypes.Dev, AppConstants.ClipTypes.Document, AppConstants.ClipTypes.Folder, AppConstants.ClipTypes.Font,
                AppConstants.ClipTypes.Generic, AppConstants.ClipTypes.Image, AppConstants.ClipTypes.Link, AppConstants.ClipTypes.System,
                AppConstants.ClipTypes.Text, AppConstants.ClipTypes.FileText, AppConstants.ClipTypes.Rtf, AppConstants.ClipTypes.Video,
            };

            FilterOptions.Clear();
            foreach (var key in orderedFilterKeys)
            {
                var icon = await _iconProvider.GetIconAsync(key, 20);
                FilterOptions.Add(new FilterOption(filterDisplayNames[key], key, icon));
            }
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _debounceTimer.Tick -= OnDebounceTimerElapsed;
                    _debounceTimer.Stop();
                    _loadClipsCts.Dispose();
                }

                _disposedValue = true;
            }
        }

        public void Dispose()
        {
            Dispose(disposing: true);
            GC.SuppressFinalize(this);
        }

    }
}