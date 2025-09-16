using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Input;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    public partial class MainViewModel
    {
        public ICommand PasteClipCommand { get; }
        public ICommand PasteClipAsPlainTextCommand { get; }
        public ICommand TransformAndPasteCommand { get; }
        public ICommand OpenSettingsCommand { get; }
        public ICommand HideWindowCommand { get; }
        public ICommand LoadMoreClipsCommand { get; }
        public ICommand TogglePinCommand { get; }
        public ICommand DeleteClipCommand { get; }
        public ICommand EditClipCommand { get; }
        public ICommand MoveToTopCommand { get; }
        public ICommand OpenCommand { get; }
        public ICommand SelectForCompareLeftCommand { get; }
        public ICommand CompareWithSelectedRightCommand { get; }
        public ICommand SendToCommand { get; }


        private async Task PerformPasteAction(ClipViewModel clipVM, Func<Clip, Task> pasteAction)
        {
            if (IsPasting) return;
            LogManager.LogDebug($"PASTE_DIAG: Initiating paste for Clip ID: {clipVM.Id}");
            IsPasting = true;
            try
            {
                if (CurrentSettings.MoveClipToTopOnPaste)
                {
                    await _clipDataService.MoveClipToTopAsync(clipVM.Id).ConfigureAwait(false);
                }

                var clip = await _clipDataService.GetClipByIdAsync(clipVM.Id).ConfigureAwait(false);
                if (clip == null) return;

                bool wasOnTop = IsAlwaysOnTop;

                if (wasOnTop)
                {
                    LogManager.LogDebug($"PASTE_DIAG: Temporarily hiding window for paste (AlwaysOnTop is active).");
                    Application.Current.MainWindow?.Hide();
                }
                else
                {
                    LogManager.LogDebug($"PASTE_DIAG: Hiding main window before paste.");
                    HideWindow();
                }

                await pasteAction(clip);
                await _clipboardService.UpdatePasteCountAsync().ConfigureAwait(false);

                await LoadClipsAsync(true);

                if (wasOnTop)
                {
                    LogManager.LogDebug($"PASTE_DIAG: Restoring main window visibility (AlwaysOnTop is active).");
                    Application.Current.MainWindow?.Show();
                    Application.Current.MainWindow?.Activate();
                }
                else
                {
                    _needsRefreshOnShow = false;
                }
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, "Paste action failed.");
                _notificationService.Show("Paste Failed", "Could not paste the selected item. The clipboard may be in use by another application.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
            finally
            {
                IsPasting = false;
            }
        }

        private async Task ExecutePasteClip(object? parameter, bool? forcePlainText)
        {
            if (parameter is ClipViewModel clipVM)
            {
                await PerformPasteAction(clipVM, clip => _pastingService.PasteClipAsync(clip, forcePlainText));
            }
        }

        private async Task ExecuteTransformAndPaste(object? parameter)
        {
            if (parameter is not object[] values || values.Length != 2) return;
            if (values[0] is not ClipViewModel clipVM || values[1] is not string transformType) return;

            await PerformPasteAction(clipVM, async clip =>
            {
                string contentToTransform = (clip.ClipType == Core.AppConstants.ClipTypes.Rtf
                    ? RtfUtils.ToPlainText(clip.Content ?? string.Empty)
                    : clip.Content) ?? string.Empty;

                var transformedContent = _clipboardService.TransformText(contentToTransform, transformType);
                await _pastingService.PasteTextAsync(transformedContent).ConfigureAwait(false);
            });
        }

        private void OpenSettingsWindow()
        {
            var settingsWindow = _serviceProvider.GetRequiredService<SettingsWindow>();
            settingsWindow.Owner = Application.Current.MainWindow;
            settingsWindow.ShowDialog();
        }

        private async Task ExecuteTogglePin(object? parameter)
        {
            if (parameter is not ClipViewModel clipVM) return;
            clipVM.IsPinned = !clipVM.IsPinned;
            LogManager.Log($"Toggling pin for clip: ID={clipVM.Id}, NewState={(clipVM.IsPinned ? "Pinned" : "Unpinned")}.");
            await _clipDataService.TogglePinAsync(clipVM.Id, clipVM.IsPinned).ConfigureAwait(false);

            if (SelectedFilter.Key == AppConstants.FilterKeys.Pinned && !clipVM.IsPinned)
            {
                Application.Current.Dispatcher.Invoke(() => Clips.Remove(clipVM));
            }
        }

        private async Task ExecuteDeleteClip(object? parameter)
        {
            if (parameter is not ClipViewModel clipVM) return;
            LogManager.Log($"Deleting clip: ID={clipVM.Id}.");
            var fullClip = await clipVM.GetFullClipAsync().ConfigureAwait(false);
            await _clipDataService.DeleteClipAsync(fullClip ?? clipVM._clip).ConfigureAwait(false);
            Application.Current.Dispatcher.Invoke(() => Clips.Remove(clipVM));
        }

        private void ExecuteEditClip(object? parameter)
        {
            if (parameter is not ClipViewModel clipVM) return;
            var wasAlwaysOnTop = this.IsAlwaysOnTop;
            if (wasAlwaysOnTop)
            {
                this.IsAlwaysOnTop = false;
            }

            var viewerViewModel = new ClipViewerViewModel(
                clipVM.Id,
                _clipDataService,
                _settingsService,
                _fontProvider,
                _serviceProvider.GetRequiredService<ISyntaxHighlighter>()
            );
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

        private async Task ExecuteMoveToTop(object? parameter)
        {
            if (parameter is not ClipViewModel clipVM) return;
            await _clipDataService.MoveClipToTopAsync(clipVM.Id);
            var clip = await _clipDataService.GetClipByIdAsync(clipVM.Id);
            if (clip != null)
            {
                await _pastingService.SetClipboardContentAsync(clip, forcePlainText: null);
            }
            await LoadClipsAsync(true);
            HideWindow();
        }

        private async Task ExecuteOpen(object? parameter)
        {
            if (parameter is not ClipViewModel clipVM) return;
            var fullClip = await clipVM.GetFullClipAsync().ConfigureAwait(false);
            if (fullClip?.Content == null) return;
            try
            {
                var path = fullClip.Content.Trim();
                if (string.IsNullOrEmpty(path)) return;
                Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
            }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or ObjectDisposedException or FileNotFoundException)
            {
                LogManager.Log(ex, $"Failed to open path: {fullClip.Content}");
                _notificationService.Show("Error", $"Could not open path: {ex.Message}", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        private void ExecuteSelectForCompareLeft(object? parameter)
        {
            if (parameter is not ClipViewModel clipVM) return;
            LeftCompareClipId = (LeftCompareClipId == clipVM.Id) ? null : clipVM.Id;
        }

        private async Task ExecuteCompareWithSelectedRight(object? parameter)
        {
            if (parameter is not ClipViewModel clipVM || !LeftCompareClipId.HasValue) return;
            var result = await _clipboardService.CompareClipsAsync(LeftCompareClipId.Value, clipVM.Id);
            if (!result.success)
            {
                _notificationService.Show("Compare Failed", result.message, ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
            LeftCompareClipId = null;
        }

        private async Task ExecuteSendTo(object? parameter)
        {
            if (parameter is not object[] values || values.Length != 2 || values[0] is not ClipViewModel clipVM || values[1] is not SendToTarget target) return;

            LogManager.LogDebug($"SENDTO_DIAG: ExecuteSendTo called for target: {target.Name} ({target.Path})");
            var clip = await clipVM.GetFullClipAsync().ConfigureAwait(false);
            if (clip?.Content == null)
            {
                LogManager.LogDebug("SENDTO_DIAG: Clip content is null, aborting.");
                return;
            }

            string contentPath;
            if (clipVM.IsFileBased)
            {
                contentPath = clip.Content.Trim();
            }
            else
            {
                var extension = clip.ClipType switch
                {
                    AppConstants.ClipTypes.CodeSnippet => ".txt",
                    AppConstants.ClipTypes.Rtf => ".rtf",
                    _ => ".txt"
                };
                var tempFilePath = Path.Combine(Path.GetTempPath(), $"cliptoo_sendto_{Guid.NewGuid()}{extension}");
                await File.WriteAllTextAsync(tempFilePath, clip.Content).ConfigureAwait(false);
                contentPath = tempFilePath;
            }

            try
            {
                string args = string.IsNullOrWhiteSpace(target.Arguments)
                    ? $"\"{contentPath}\""
                    : string.Format(System.Globalization.CultureInfo.InvariantCulture, target.Arguments, contentPath);
                Process.Start(new ProcessStartInfo(target.Path, args) { UseShellExecute = true });
            }
            catch (Exception ex) when (ex is IOException or System.ComponentModel.Win32Exception or UnauthorizedAccessException or PlatformNotSupportedException)
            {
                LogManager.Log(ex, $"Failed to send to path: {target.Path} with content {contentPath}");
                _notificationService.Show("Error", $"Could not send to '{target.Name}'.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }
    }
}