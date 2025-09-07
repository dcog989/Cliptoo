using System.Windows;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;

namespace Cliptoo.UI.ViewModels
{
    public partial class MainViewModel
    {
        private uint _currentOffset;
        private bool _isLoadingMore;
        private const uint PageSize = 50;
        private CancellationTokenSource _loadClipsCts = new();
        private bool _canLoadMore = true;

        public async Task LoadClipsAsync(bool scrollToTop = false)
        {
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            LogManager.LogDebug($"LoadClipsAsync called. Search='{SearchTerm}', Filter='{SelectedFilter?.Key ?? AppConstants.FilterKeys.All}', IsReadyForEvents: {IsReadyForEvents}");
            if (!_isReadyForEvents)
            {
                return;
            }
            await _loadClipsCts.CancelAsync();
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

                var clipsData = await _clipDataService.GetClipsAsync(limit: PageSize, offset: _currentOffset, searchTerm: localSearchTerm, filterType: localFilterKey, cancellationToken: token);

                if (clipsData.Count < PageSize)
                {
                    _canLoadMore = false;
                }

                if (token.IsCancellationRequested) return;

                await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    if (token.IsCancellationRequested) return;

                    Clips.Clear();
                    var theme = CurrentThemeString;
                    foreach (var clip in clipsData)
                    {
                        var vm = _clipViewModelFactory.Create(clip, CurrentSettings, theme, this);
                        ApplyAppearanceToViewModel(vm);
                        Clips.Add(vm);
                    }

                    if (scrollToTop)
                    {
                        ListScrolledToTopRequest?.Invoke(this, EventArgs.Empty);
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

                var clipsData = await _clipDataService.GetClipsAsync(limit: PageSize, offset: _currentOffset, searchTerm: localSearchTerm, filterType: localFilterKey, cancellationToken: token);

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

        public void RefreshClipList()
        {
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
    }
}