using System.Diagnostics;
using System.Windows;
using System.Windows.Input;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Services;
using Cliptoo.Core.Configuration;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Wpf.Ui.Controls;
using Cliptoo.UI.Helpers;

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

                var clip = await _clipDataService.GetClipByIdAsync(clipVM.Id);
                if (clip == null) return;

                var stopwatch = Stopwatch.StartNew();
                while ((Core.Native.KeyboardUtils.IsControlPressed() || Core.Native.KeyboardUtils.IsAltPressed()) && stopwatch.ElapsedMilliseconds < 500)
                {
                    await Task.Delay(20);
                }
                stopwatch.Stop();
                LogManager.LogDebug($"PASTE_DIAG: Modifier key check completed in {stopwatch.ElapsedMilliseconds}ms.");

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
                await _clipboardService.UpdatePasteCountAsync();

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

        private void ShowClipEditor(ClipViewModel clipVM)
        {
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
    }
}