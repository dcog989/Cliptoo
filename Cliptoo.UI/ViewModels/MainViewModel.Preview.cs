using System.Windows;
using System.Windows.Threading;
using Cliptoo.UI.Helpers;

namespace Cliptoo.UI.ViewModels
{
    public partial class MainViewModel
    {
        private readonly DispatcherTimer _showPreviewTimer;
        private readonly DispatcherTimer _hidePreviewTimer;
        private WeakReference<ClipViewModel>? _previewClipRef;
        private bool _isPreviewOpen;

        public ClipViewModel? PreviewClip => _previewClipRef != null && _previewClipRef.TryGetTarget(out var target) ? target : null;
        public bool IsPreviewOpen { get => _isPreviewOpen; set => SetProperty(ref _isPreviewOpen, value); }

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

            if (CurrentSettings.HoverPreviewDelay == 0)
            {
                return;
            }

            PreviewPlacementMode = System.Windows.Controls.Primitives.PlacementMode.Mouse;
            PreviewPlacementTarget = null;
            OnPropertyChanged(nameof(PreviewPlacementMode));
            OnPropertyChanged(nameof(PreviewPlacementTarget));

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

            var loadTasks = new System.Collections.Generic.List<Task>
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
                PreviewPlacementTarget = null;
                OnPropertyChanged(nameof(PreviewPlacementTarget));
            }
        }

        public void TogglePreviewForSelection(UIElement? placementTarget)
        {
            var listView = (System.Windows.Application.Current.MainWindow as Views.MainWindow)?.ClipListView;
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

                PreviewPlacementMode = System.Windows.Controls.Primitives.PlacementMode.Right;
                PreviewPlacementTarget = placementTarget;
                OnPropertyChanged(nameof(PreviewPlacementMode));
                OnPropertyChanged(nameof(PreviewPlacementTarget));

                _previewClipRef = new WeakReference<ClipViewModel>(selectedVm);
                OnShowPreviewTimerTick(null, EventArgs.Empty);
            }
        }
    }
}