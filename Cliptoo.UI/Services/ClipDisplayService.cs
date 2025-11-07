using System.Collections.ObjectModel;
using System.Windows;
using System.Windows.Threading;
using Cliptoo.Core;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.UI.ViewModels;
using Cliptoo.UI.Helpers;

namespace Cliptoo.UI.Services
{
    public class ClipDisplayService : IClipDisplayService, IDisposable
    {
        private readonly IClipDataService _clipDataService;
        private readonly IClipViewModelFactory _clipViewModelFactory;
        private readonly IIconProvider _iconProvider;
        private readonly ISettingsService _settingsService;

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
            _settingsService = settingsService;
            _iconProvider = iconProvider;

            _selectedFilter = new FilterOption("All", AppConstants.FilterKeyAll, null);

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
            SelectedFilter = FilterOptions.FirstOrDefault() ?? new FilterOption("All", AppConstants.FilterKeyAll, null);
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
                string localFilterKey = SelectedFilter?.Key ?? AppConstants.FilterKeyAll;
                string tagSearchPrefix = _settingsService.Settings.TagSearchPrefix;

                if (string.IsNullOrEmpty(localFilterKey)) localFilterKey = AppConstants.FilterKeyAll;
                if (!string.IsNullOrEmpty(localSearchTerm) && !localSearchTerm.StartsWith(tagSearchPrefix, StringComparison.Ordinal) && localSearchTerm.Length < 2)
                {
                    _currentOffset = 0;
                    return;
                }
                _currentOffset = 0;

                var clipsData = await _clipDataService.GetClipsAsync(PageSize, _currentOffset, localSearchTerm, localFilterKey, tagSearchPrefix, token);
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
            string tagSearchPrefix = _settingsService.Settings.TagSearchPrefix;
            if (!string.IsNullOrEmpty(SearchTerm) && !SearchTerm.StartsWith(tagSearchPrefix, StringComparison.Ordinal) && SearchTerm.Length < 2) return;

            var token = _loadClipsCts.Token;
            _isLoadingMore = true;
            try
            {
                var clipsData = await _clipDataService.GetClipsAsync(PageSize, _currentOffset, SearchTerm, SelectedFilter.Key, tagSearchPrefix, token);
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

        public void HandleNewClip(Clip newClip)
        {
            ArgumentNullException.ThrowIfNull(newClip);

            // If a search is active, we must check if the new clip matches before adding it.
            if (!string.IsNullOrEmpty(SearchTerm))
            {
                string tagSearchPrefix = _settingsService.Settings.TagSearchPrefix;
                bool isTagSearch = SearchTerm.StartsWith(tagSearchPrefix, StringComparison.Ordinal);
                string actualSearchTerm = isTagSearch ? SearchTerm.Substring(tagSearchPrefix.Length) : SearchTerm;

                // Only proceed if there's a real search term to check against.
                if (!string.IsNullOrWhiteSpace(actualSearchTerm))
                {
                    var searchWords = actualSearchTerm.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
                    bool matchesSearch;

                    if (isTagSearch)
                    {
                        matchesSearch = !string.IsNullOrEmpty(newClip.Tags) &&
                                        searchWords.All(word => newClip.Tags.Contains(word, StringComparison.OrdinalIgnoreCase));
                    }
                    else
                    {
                        string contentForSearch = newClip.PreviewContent ?? string.Empty;
                        if (newClip.ClipType == AppConstants.ClipTypeRtf)
                        {
                            contentForSearch = RtfUtils.ToPlainText(contentForSearch);
                        }
                        matchesSearch = searchWords.All(word => contentForSearch.Contains(word, StringComparison.OrdinalIgnoreCase));
                    }

                    if (!matchesSearch)
                    {
                        LogManager.LogDebug($"New clip does not match active search term '{SearchTerm}'. UI not updated.");
                        return; // Don't add if it doesn't match the search.
                    }
                }
            }

            bool matchesFilter = SelectedFilter.Key == AppConstants.FilterKeyAll ||
                                (SelectedFilter.Key == AppConstants.ClipTypeLink && (newClip.ClipType == AppConstants.ClipTypeLink || newClip.ClipType == AppConstants.ClipTypeFileLink)) ||
                                SelectedFilter.Key == newClip.ClipType;

            // A new clip cannot be a favorite, so if that filter is active, don't add.
            if (SelectedFilter.Key == AppConstants.FilterKeyFavorite)
            {
                matchesFilter = false;
            }

            if (!matchesFilter)
            {
                // New clip doesn't match the current view, do nothing.
                LogManager.LogDebug($"New clip (type: {newClip.ClipType}) does not match active filter ({SelectedFilter.Key}). UI not updated.");
                return;
            }

            Application.Current.Dispatcher.Invoke(() =>
            {
                var theme = MainViewModel.CurrentThemeString;
                var viewModel = _clipViewModelFactory.Create(newClip, theme);
                Clips.Insert(0, viewModel);
                _currentOffset++; // Increment offset to keep paging correct if user scrolls down later.
            });
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
                { AppConstants.FilterKeyAll, "All" }, { AppConstants.FilterKeyFavorite, "Favorites" },
                { AppConstants.ClipTypeArchive, "Archives" }, { AppConstants.ClipTypeAudio, "Audio" },
                { AppConstants.ClipTypeDev, "Dev Files" }, { AppConstants.ClipTypeCodeSnippet, "Code Snippets" },
                { AppConstants.ClipTypeColor, "Colors" }, { AppConstants.ClipTypeDanger, "Dangerous" },
                { AppConstants.ClipTypeDatabase, "Database Files" }, { AppConstants.ClipTypeDocument, "Documents" },
                { AppConstants.ClipTypeFileText, "Text Files" }, { AppConstants.ClipTypeFolder, "Folders" },
                { AppConstants.ClipTypeFont, "Font Files" }, { AppConstants.ClipTypeGeneric, "Generic Files" },
                { AppConstants.ClipTypeImage, "Images" }, { AppConstants.ClipTypeLink, "Links" },
                { AppConstants.ClipTypeRtf, "Formatted Text" }, { AppConstants.ClipTypeSystem, "System Files" },
                { AppConstants.ClipTypeText, "Text" }, { AppConstants.ClipTypeVideo, "Video" },
            };
            var orderedFilterKeys = new[]
            {
                AppConstants.FilterKeyAll, AppConstants.FilterKeyFavorite, AppConstants.ClipTypeArchive, AppConstants.ClipTypeAudio,
                AppConstants.ClipTypeCodeSnippet, AppConstants.ClipTypeColor, AppConstants.ClipTypeDanger, AppConstants.ClipTypeDatabase,
                AppConstants.ClipTypeDev, AppConstants.ClipTypeDocument, AppConstants.ClipTypeFolder, AppConstants.ClipTypeFont,
                AppConstants.ClipTypeGeneric, AppConstants.ClipTypeImage, AppConstants.ClipTypeLink, AppConstants.ClipTypeSystem,
                AppConstants.ClipTypeText, AppConstants.ClipTypeFileText, AppConstants.ClipTypeRtf, AppConstants.ClipTypeVideo,
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