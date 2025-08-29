using System.Diagnostics;
using System.Windows;
using System.Windows.Input;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Services;
using Cliptoo.UI.Services;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    public partial class MainViewModel
    {
        public ICommand PasteClipCommand { get; }
        public ICommand OpenSettingsCommand { get; }
        public ICommand HideWindowCommand { get; }
        public ICommand LoadMoreClipsCommand { get; }

        private async Task PerformPasteAction(ClipViewModel clipVM, Func<Clip, Task> pasteAction)
        {
            if (IsPasting) return;

            IsPasting = true;
            try
            {
                var clip = await _clipDataService.GetClipByIdAsync(clipVM.Id);
                if (clip == null) return;

                var stopwatch = Stopwatch.StartNew();
                while ((Core.Native.KeyboardUtils.IsControlPressed() || Core.Native.KeyboardUtils.IsAltPressed()) && stopwatch.ElapsedMilliseconds < 500)
                {
                    await Task.Delay(20);
                }

                HideWindow();

                await pasteAction(clip);
                await _clipboardService.UpdatePasteCountAsync();

                await LoadClipsAsync(true);
            }
            catch (Exception ex)
            {
                Core.Configuration.LogManager.Log(ex, "Paste action failed.");
                _notificationService.Show("Paste Failed", "Could not paste the selected item. The clipboard may be in use by another application.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
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