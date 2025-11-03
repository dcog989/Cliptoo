using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Input;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Views;
using Microsoft.Data.Sqlite;
using Microsoft.Extensions.DependencyInjection;
using Wpf.Ui;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    public partial class MainViewModel
    {
        // The delay (in ms) to wait after showing the "Copied" notification
        // from a tray activation before hiding the main window. This gives the user time to read the message.
        private const int HideWindowAfterNotificationDelayMs = 5000;

        public ICommand PasteClipCommand { get; }
        public ICommand OpenSettingsCommand { get; }
        public ICommand HideWindowCommand { get; }
        public ICommand LoadMoreClipsCommand { get; }

        private async Task PerformPasteAction(ClipViewModel clipVM, Func<Clip, Task> pasteAction)
        {
            if (IsPasting) return;
            LogManager.LogDebug($"PASTE_DIAG: Initiating paste for Clip ID: {clipVM.Id}");
            IsPasting = true;
            try
            {
                if (CurrentSettings.MoveClipToTopOnPaste)
                {
                    await _clipDataService.MoveClipToTopAsync(clipVM.Id);
                }

                var clip = await _clipDataService.GetClipByIdAsync(clipVM.Id);
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
                await _clipDataService.IncrementPasteCountAsync(clip.Id);
                await _clipboardService.UpdatePasteCountAsync();

                await _clipDisplayService.LoadClipsAsync(true);

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
            catch (Exception ex) when (ex is IOException or System.ComponentModel.Win32Exception or SqliteException or InvalidOperationException)
            {
                LogManager.LogError($"Paste action failed. Error: {ex.Message}");
                _notificationService.Show("Paste Failed", "Could not paste the selected item. The clipboard may be in use by another application.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
            finally
            {
                IsPasting = false;
            }
        }

        private async Task ExecutePasteClip(object? parameter, bool? forcePlainText)
        {
            if (parameter is not ClipViewModel clipVM) return;

            if (ActivationSourceIsTray)
            {
                await CopyToClipboardAndNotify(clipVM, forcePlainText);
            }
            else
            {
                await PerformPasteAction(clipVM, clip => _pastingService.PasteClipAsync(clip, forcePlainText));
            }
        }

        private async Task CopyToClipboardAndNotify(ClipViewModel clipVM, bool? forcePlainText)
        {
            if (IsPasting) return;
            IsPasting = true;
            try
            {
                if (CurrentSettings.MoveClipToTopOnPaste)
                {
                    await _clipDataService.MoveClipToTopAsync(clipVM.Id);
                    _needsRefreshOnShow = true; // Refresh needed to show moved clip next time
                }

                var clip = await _clipDataService.GetClipByIdAsync(clipVM.Id);
                if (clip == null) return;

                await _pastingService.SetClipboardContentAsync(clip, forcePlainText);
                await _clipDataService.IncrementPasteCountAsync(clip.Id);
                await _clipboardService.UpdatePasteCountAsync();

                var notificationTimeoutSeconds = (int)Math.Ceiling(HideWindowAfterNotificationDelayMs / 1000.0);
                _notificationService.Show("Copied to Clipboard", "Ready to paste in your target application.", ControlAppearance.Primary, SymbolRegular.Copy24, notificationTimeoutSeconds);

                await Task.Delay(HideWindowAfterNotificationDelayMs);
                HideWindow();
            }
            catch (Exception ex) when (ex is IOException or System.ComponentModel.Win32Exception or SqliteException or InvalidOperationException)
            {
                LogManager.LogError($"Copy-to-clipboard action failed. Error: {ex.Message}");
                _notificationService.Show("Copy Failed", "Could not copy the selected item.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
            finally
            {
                IsPasting = false;
            }
        }

        private async Task ExecuteTransformAndPaste(int clipId, string transformType)
        {
            var clipVM = Clips.FirstOrDefault(c => c.Id == clipId);
            if (clipVM == null) return;

            await PerformPasteAction(clipVM, async clip =>
            {
                string contentToTransform = (clip.ClipType == AppConstants.ClipTypeRtf
                    ? RtfUtils.ToPlainText(clip.Content ?? string.Empty)
                    : clip.Content) ?? string.Empty;

                var transformedContent = _clipboardService.TransformText(contentToTransform, transformType);
                await _pastingService.PasteTextAsync(transformedContent);
            });
        }

        private void OpenSettingsWindow()
        {
            var settingsWindow = _serviceProvider.GetRequiredService<SettingsWindow>();
            settingsWindow.Owner = Application.Current.MainWindow;
            settingsWindow.ShowDialog();
        }

        private async Task ExecuteToggleFavorite(int clipId, bool isFavorite)
        {
            var clipVM = Clips.FirstOrDefault(c => c.Id == clipId);
            if (clipVM == null) return;

            clipVM.IsFavorite = isFavorite;
            LogManager.LogInfo($"Toggling favorite for clip: ID={clipVM.Id}, NewState={(clipVM.IsFavorite ? "Favorite" : "Not Favorite")}.");
            await _clipDataService.ToggleFavoriteAsync(clipVM.Id, clipVM.IsFavorite);

            if (SelectedFilter.Key == AppConstants.FilterKeyFavorite && !clipVM.IsFavorite)
            {
                Application.Current.Dispatcher.Invoke(() => Clips.Remove(clipVM));
            }
        }

        private async Task ExecuteDeleteClip(int clipId)
        {
            var clipVM = Clips.FirstOrDefault(c => c.Id == clipId);
            if (clipVM == null || clipVM.IsDeleting) return;

            LogManager.LogInfo($"Deleting clip: ID={clipVM.Id}.");
            clipVM.IsDeleting = true;

            await Task.Delay(300);

            try
            {
                var fullClip = await clipVM.GetFullClipAsync();
                await _clipDataService.DeleteClipAsync(fullClip ?? clipVM._clip);

                await Application.Current.Dispatcher.InvokeAsync(() => Clips.Remove(clipVM));
            }
            catch (Exception ex) when (ex is SqliteException or IOException)
            {
                LogManager.LogError($"Failed to delete clip ID={clipVM.Id}. Error: {ex.Message}");

                await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    _notificationService.Show("Delete Failed", "Could not delete the selected item.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
                    clipVM.IsDeleting = false;
                });
            }
        }

        private void ExecuteEditClip(int clipId)
        {
            var clipVM = Clips.FirstOrDefault(c => c.Id == clipId);
            if (clipVM == null) return;

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
                _serviceProvider.GetRequiredService<ISyntaxHighlighter>(),
                _serviceProvider.GetRequiredService<IContentDialogService>()
            );
            viewerViewModel.OnClipUpdated += (_, _) => _clipDisplayService.RefreshClipList();

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

        private async Task ExecuteMoveToTop(int clipId)
        {
            var clipVM = Clips.FirstOrDefault(c => c.Id == clipId);
            if (clipVM == null) return;

            // Trigger the animation and wait for it to complete.
            clipVM.IsMoving = true;
            await Task.Delay(300);

            // Perform the data operation.
            await _clipDataService.MoveClipToTopAsync(clipId);

            // Get the updated clip data (with new timestamp).
            var updatedClip = await _clipDataService.GetPreviewClipByIdAsync(clipId);

            // Update the UI collection on the UI thread.
            Clips.Remove(clipVM);

            if (updatedClip != null)
            {
                // Create a new view model for the updated clip data to avoid state issues.
                var newClipVM = _clipViewModelFactory.Create(updatedClip, CurrentThemeString);
                Clips.Insert(0, newClipVM);

                // Ensure the new top item is visible and selected.
                var mainWindow = Application.Current.MainWindow as MainWindow;
                var listView = mainWindow?.ClipListViewControl;
                if (listView != null)
                {
                    listView.SelectedItem = newClipVM;
                    listView.ScrollIntoView(newClipVM);
                }
            }
            else
            {
                // Failsafe in case we can't get the updated clip for some reason.
                _clipDisplayService.RefreshClipList();
            }

            // Also update the system clipboard to reflect the moved item.
            var fullClip = await _clipDataService.GetClipByIdAsync(clipId);
            if (fullClip != null)
            {
                await _pastingService.SetClipboardContentAsync(fullClip, forcePlainText: null);
            }
        }

        private async Task ExecuteOpen(int clipId)
        {
            var clipVM = Clips.FirstOrDefault(c => c.Id == clipId);
            if (clipVM == null) return;

            var fullClip = await clipVM.GetFullClipAsync();
            if (fullClip?.Content == null) return;
            try
            {
                var path = fullClip.Content.Trim();
                if (string.IsNullOrEmpty(path)) return;
                Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
            }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or ObjectDisposedException or FileNotFoundException)
            {
                LogManager.LogError($"Failed to open path: {fullClip.Content}. Error: {ex.Message}");
                _notificationService.Show("Error", $"Could not open path: {ex.Message}", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        private void ExecuteSelectForCompareLeft(int clipId)
        {
            _comparisonStateService.SelectLeftClip(clipId);
        }

        private async Task ExecuteCompareWithSelectedRight(int clipId)
        {
            var result = await _comparisonStateService.CompareWithRightClipAsync(clipId);
            if (!result.success)
            {
                _notificationService.Show("Compare Failed", result.message, ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        private async Task ExecuteSendTo(int clipId, SendToTarget target)
        {
            var clipVM = Clips.FirstOrDefault(c => c.Id == clipId);
            if (clipVM == null || target == null) return;

            LogManager.LogDebug($"SENDTO_DIAG: ExecuteSendTo called for target: {target.Name} ({target.Path})");
            var clip = await clipVM.GetFullClipAsync();
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
                    AppConstants.ClipTypeCodeSnippet => ".txt",
                    AppConstants.ClipTypeRtf => ".rtf",
                    _ => ".txt"
                };
                var tempFilePath = Path.Combine(Path.GetTempPath(), $"cliptoo_sendto_{Guid.NewGuid()}{extension}");
                await File.WriteAllTextAsync(tempFilePath, clip.Content);
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
                LogManager.LogError($"Failed to send to path: {target.Path} with content {contentPath}. Error: {ex.Message}");
                _notificationService.Show("Error", $"Could not send to '{target.Name}'.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

    }
}