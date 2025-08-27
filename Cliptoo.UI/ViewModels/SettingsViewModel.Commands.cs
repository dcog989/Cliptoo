using System.Globalization;
using System.IO;
using System.Windows;
using System.Windows.Input;
using Cliptoo.Core.Configuration;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Win32;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    internal partial class SettingsViewModel
    {
        public ICommand SaveSettingsCommand { get; }
        public ICommand ClearHistoryCommand { get; }
        public ICommand ClearCachesCommand { get; }
        public ICommand RunHeavyMaintenanceCommand { get; }
        public ICommand RemoveDeadheadClipsCommand { get; }
        public ICommand ClearOversizedCommand { get; }
        public ICommand ChangePageCommand { get; }
        public ICommand OpenGitHubUrlCommand { get; }
        public ICommand OpenSettingsFolderCommand { get; }
        public ICommand OpenTempDataFolderCommand { get; }
        public ICommand OpenAcknowledgementsWindowCommand { get; }
        public ICommand OpenExeFolderCommand { get; }
        public ICommand BrowseCompareToolCommand { get; }
        public ICommand AddSendToTargetCommand { get; }
        public ICommand RemoveSendToTargetCommand { get; }

        private void ExecuteBrowseCompareTool()
        {
            var openFileDialog = new OpenFileDialog
            {
                Filter = "Executable files (*.exe)|*.exe|All files (*.*)|*.*",
                Title = "Select a comparison program"
            };

            if (openFileDialog.ShowDialog() == true)
            {
                CompareToolPath = openFileDialog.FileName;
            }
        }

        private async Task HandleClearHistory()
        {
            var viewModel = new ClearHistoryDialogViewModel();
            var dialog = new ContentDialog
            {
                Title = "Clear History?",
                Content = new Views.ClearHistoryDialog { DataContext = viewModel },
                PrimaryButtonText = "Delete",
                CloseButtonText = "Cancel"
            };

            var result = await _contentDialogService.ShowAsync(dialog, CancellationToken.None);

            if (result != ContentDialogResult.Primary)
            {
                viewModel.Result = ClearHistoryResult.Cancel;
            }
            else
            {
                viewModel.Result = viewModel.DeletePinned ? ClearHistoryResult.ClearAll : ClearHistoryResult.ClearUnpinned;
            }

            if (viewModel.Result == ClearHistoryResult.Cancel) return;

            if (IsBusy) return;
            IsBusy = true;
            try
            {
                if (viewModel.Result == ClearHistoryResult.ClearAll)
                {
                    await Task.Run(async () => await _controller.ClearAllHistoryAsync().ConfigureAwait(false)).ConfigureAwait(true);
                }
                else if (viewModel.Result == ClearHistoryResult.ClearUnpinned)
                {
                    await Task.Run(async () => await _controller.ClearHistoryAsync().ConfigureAwait(false)).ConfigureAwait(true);
                }
                await InitializeAsync();
            }
            finally
            {
                IsBusy = false;
            }
        }

        private async Task HandleClearOversized()
        {
            var viewModel = new ClearOversizedDialogViewModel();
            var dialog = new ContentDialog
            {
                Title = "Clear Oversized Clips?",
                Content = new Views.ClearOversizedDialog { DataContext = viewModel },
                PrimaryButtonText = "Delete",
                CloseButtonText = "Cancel"
            };

            var result = await _contentDialogService.ShowAsync(dialog, CancellationToken.None);

            if (result != ContentDialogResult.Primary)
            {
                return;
            }

            if (IsBusy) return;
            IsBusy = true;
            try
            {
                int count = await Task.Run(async () => await _controller.ClearOversizedClipsAsync(viewModel.SizeMb).ConfigureAwait(false)).ConfigureAwait(true);
                await InitializeAsync();
                await ShowInformationDialogAsync("Oversized Clips Removed", new System.Windows.Controls.TextBlock { Text = string.Format(CultureInfo.CurrentCulture, "{0} clip(s) larger than {1} MB have been removed.", count, viewModel.SizeMb) });
            }
            finally
            {
                IsBusy = false;
            }
        }

        private async Task HandleRemoveDeadheadClips()
        {
            if (IsBusy) return;
            IsBusy = true;
            try
            {
                int count = await Task.Run(async () => await _controller.RemoveDeadheadClipsAsync().ConfigureAwait(false)).ConfigureAwait(true);
                await InitializeAsync();
                await ShowInformationDialogAsync("Deadhead Clips Removed", new System.Windows.Controls.TextBlock { Text = string.Format(CultureInfo.CurrentCulture, "{0} clip(s) pointing to non-existent files or folders have been removed.", count) });
            }
            finally
            {
                IsBusy = false;
            }
        }

        private async Task ShowInformationDialogAsync(string title, UIElement content)
        {
            var dialog = new ContentDialog
            {
                Title = title,
                Content = content,
                CloseButtonText = "OK"
            };
            await _contentDialogService.ShowAsync(dialog, CancellationToken.None);
        }

        private void ShowAcknowledgementsWindow()
        {
            var window = _serviceProvider.GetRequiredService<AcknowledgementsWindow>();
            window.Owner = Application.Current.Windows.OfType<SettingsWindow>().FirstOrDefault();
            window.ShowDialog();
        }

        private void ExecuteAddSendToTarget()
        {
            var openFileDialog = new OpenFileDialog
            {
                Filter = "Executable files (*.exe)|*.exe|All files (*.*)|*.*",
                Title = "Select an Application"
            };

            if (openFileDialog.ShowDialog() == true)
            {
                var newTarget = new SendToTarget
                {
                    Path = openFileDialog.FileName,
                    Name = Path.GetFileNameWithoutExtension(openFileDialog.FileName),
                    Arguments = "\"{0}\""
                };
                SendToTargets.Add(newTarget);
            }
        }

        private void ExecuteRemoveSendToTarget(SendToTarget? target)
        {
            if (target != null)
            {
                SendToTargets.Remove(target);
            }
        }
    }
}