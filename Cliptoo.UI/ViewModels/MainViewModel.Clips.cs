using System.Windows;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;

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
            Cliptoo.Core.Configuration.LogManager.Log($"DIAG_LOAD: LoadClipsAsync called. IsReadyForEvents: {IsReadyForEvents}");
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

                    SyncClipsCollection(clipsData);

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

        private void SyncClipsCollection(List<Clip> newClips)
        {
            var theme = CurrentThemeString;
            var vmMap = Clips.ToDictionary(vm => vm.Id);
            var newClipIds = newClips.Select(c => c.Id).ToHashSet();

            // First, remove VMs that are no longer in the new list of clips.
            // Iterating backwards is important when removing items from a collection.
            for (int i = Clips.Count - 1; i >= 0; i--)
            {
                if (!newClipIds.Contains(Clips[i].Id))
                {
                    Clips.RemoveAt(i);
                }
            }

            // Now, add, move, and update existing items to match the new list order.
            for (int i = 0; i < newClips.Count; i++)
            {
                var clip = newClips[i];
                ClipViewModel vm;

                if (vmMap.TryGetValue(clip.Id, out var existingVm))
                {
                    vm = existingVm;
                    vm.UpdateClip(clip, theme);
                }
                else
                {
                    vm = _clipViewModelFactory.Create(clip, CurrentSettings, theme, this);
                    ApplyAppearanceToViewModel(vm);
                }

                // If the item is already at the correct position, do nothing.
                if (i < Clips.Count && Clips[i].Id == vm.Id)
                {
                    continue;
                }

                // The item is not in the correct position, so we need to move or insert it.
                int oldIndex = Clips.IndexOf(vm);

                if (oldIndex >= 0)
                {
                    // The item exists in the collection but at the wrong place. Move it.
                    Clips.Move(oldIndex, i);
                }
                else
                {
                    // The item is new to the collection. Insert it.
                    Clips.Insert(i, vm);
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
    }
}