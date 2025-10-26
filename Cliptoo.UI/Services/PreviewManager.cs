using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Threading;
using Cliptoo.Core.Interfaces;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.ViewModels;
using Cliptoo.UI.ViewModels.Base;

namespace Cliptoo.UI.Services
{
    public class PreviewManager : ViewModelBase, IPreviewManager
    {
        private readonly ISettingsService _settingsService;
        private readonly DispatcherTimer _showPreviewTimer;
        private readonly DispatcherTimer _hidePreviewTimer;
        private WeakReference<ClipViewModel>? _previewClipRef;

        private bool _isPreviewOpen;
        public bool IsPreviewOpen { get => _isPreviewOpen; set => SetProperty(ref _isPreviewOpen, value); }

        public ClipViewModel? PreviewClip => _previewClipRef != null && _previewClipRef.TryGetTarget(out var target) ? target : null;

        private System.Windows.Controls.Primitives.PlacementMode _placementMode = System.Windows.Controls.Primitives.PlacementMode.Mouse;
        public System.Windows.Controls.Primitives.PlacementMode PlacementMode { get => _placementMode; private set => SetProperty(ref _placementMode, value); }

        private UIElement? _placementTarget;
        public UIElement? PlacementTarget { get => _placementTarget; private set => SetProperty(ref _placementTarget, value); }

        public PreviewManager(ISettingsService settingsService)
        {
            _settingsService = settingsService;

            _showPreviewTimer = new DispatcherTimer();
            _showPreviewTimer.Tick += OnShowPreviewTimerTick;

            _hidePreviewTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(100) };
            _hidePreviewTimer.Tick += OnHidePreviewTimerTick;
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

            if (_settingsService.Settings.HoverPreviewDelay == 0)
            {
                return;
            }

            PlacementMode = System.Windows.Controls.Primitives.PlacementMode.Mouse;
            PlacementTarget = null;

            _previewClipRef = new WeakReference<ClipViewModel>(clipVm);
            _showPreviewTimer.Interval = TimeSpan.FromMilliseconds(_settingsService.Settings.HoverPreviewDelay);
            _showPreviewTimer.Start();
        }

        public void RequestHidePreview()
        {
            _showPreviewTimer.Stop();
            _hidePreviewTimer.Start();
        }

        public void TogglePreviewForSelection(ClipViewModel selectedVm, UIElement? placementTarget)
        {
            if (IsPreviewOpen && PreviewClip?.Id == selectedVm.Id)
            {
                RequestHidePreview();
            }
            else
            {
                _showPreviewTimer.Stop();
                _hidePreviewTimer.Stop();

                PlacementMode = System.Windows.Controls.Primitives.PlacementMode.Right;
                PlacementTarget = placementTarget;

                _previewClipRef = new WeakReference<ClipViewModel>(selectedVm);
                OnShowPreviewTimerTick(null, EventArgs.Empty);
            }
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
                loadTasks.Add(currentPreviewClip.LoadImagePreviewAsync(_settingsService.Settings.HoverImagePreviewSize));
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
                PlacementTarget = null;
            }
        }
    }
}