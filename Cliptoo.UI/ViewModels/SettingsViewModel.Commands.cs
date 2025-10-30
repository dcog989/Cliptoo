using System.Globalization;
using System.IO;
using System.Text.Json;
using System.Windows;
using System.Windows.Input;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Logging;
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
        public ICommand MoveSendToTargetUpCommand { get; }
        public ICommand MoveSendToTargetDownCommand { get; }
        public ICommand ExportAllCommand { get; }
        public ICommand ExportFavoriteCommand { get; }
        public ICommand ImportCommand { get; }
        public ICommand AddBlacklistedAppCommand { get; }
        public ICommand RemoveBlacklistedAppCommand { get; }
        public ICommand BrowseAndAddBlacklistedAppCommand { get; }

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
                int count = await _databaseService.ClearOversizedClipsAsync(viewModel.SizeMb);
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
                int count = await _databaseService.RemoveDeadheadClipsAsync();
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

        private void ShowAcknowledgementsWindow(object? ownerWindow)
        {
            var window = _serviceProvider.GetRequiredService<AcknowledgementsWindow>();
            if (ownerWindow is Window owner)
            {
                window.Owner = owner;
            }
            window.ShowDialog();
        }

        private async Task ExecuteExport(bool favoriteOnly)
        {
            string downloadsPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads");
            if (!Directory.Exists(downloadsPath))
            {
                // Fallback to My Documents if Downloads folder doesn't exist
                downloadsPath = Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments);
            }

            string timestamp = DateTime.Now.ToString("yyyyMMdd-HHmmss", CultureInfo.InvariantCulture);
            string baseFileName = favoriteOnly ? "cliptoo_favorites_export" : "cliptoo_export";
            string fileName = $"{baseFileName}_{timestamp}.json";

            var dialog = new SaveFileDialog
            {
                Filter = "JSON files (*.json)|*.json",
                Title = favoriteOnly ? "Export Favorite Clips" : "Export All Clips",
                InitialDirectory = downloadsPath,
                FileName = fileName
            };

            if (dialog.ShowDialog() != true) return;

            IsBusy = true;
            try
            {
                var jsonContent = await _databaseService.ExportToJsonStringAsync(favoriteOnly).ConfigureAwait(true);
                await File.WriteAllTextAsync(dialog.FileName, jsonContent).ConfigureAwait(true);
                await ShowInformationDialogAsync("Export Complete", new System.Windows.Controls.TextBlock { Text = $"Successfully exported clips to:\n{dialog.FileName}" });
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.LogCritical(ex, "Failed to export clips.");
                await ShowInformationDialogAsync("Export Failed", new System.Windows.Controls.TextBlock { Text = $"An error occurred during export: {ex.Message}" });
            }
            finally
            {
                IsBusy = false;
            }
        }

        private async Task ExecuteImport()
        {
            var dialog = new OpenFileDialog
            {
                Filter = "JSON files (*.json)|*.json",
                Title = "Import Clips"
            };

            if (dialog.ShowDialog() != true) return;

            var confirmDialog = new ContentDialog
            {
                Title = "Confirm Import",
                Content = "This will import clips from the selected file. Duplicates based on content will be ignored. Continue?",
                PrimaryButtonText = "Import",
                CloseButtonText = "Cancel"
            };

            var result = await _contentDialogService.ShowAsync(confirmDialog, CancellationToken.None);
            if (result != ContentDialogResult.Primary) return;

            IsBusy = true;
            try
            {
                var jsonContent = await File.ReadAllTextAsync(dialog.FileName).ConfigureAwait(true);
                int importedCount = await _databaseService.ImportFromJsonAsync(jsonContent).ConfigureAwait(true);
                await InitializeAsync().ConfigureAwait(true);
                await ShowInformationDialogAsync("Import Complete", new System.Windows.Controls.TextBlock { Text = $"{importedCount} new clips were successfully imported." });
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException)
            {
                LogManager.LogCritical(ex, "Failed to import clips.");
                await ShowInformationDialogAsync("Import Failed", new System.Windows.Controls.TextBlock { Text = $"An error occurred during import. The file may be corrupt or in an invalid format.\n\nError: {ex.Message}" });
            }
            finally
            {
                IsBusy = false;
            }
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

        private bool CanExecuteMoveSendToTargetUp(object? parameter)
        {
            if (parameter is SendToTarget target)
            {
                return SendToTargets.IndexOf(target) > 0;
            }
            return false;
        }

        private void ExecuteMoveSendToTargetUp(object? parameter)
        {
            if (parameter is SendToTarget target)
            {
                int index = SendToTargets.IndexOf(target);
                if (index > 0)
                {
                    SendToTargets.Move(index, index - 1);
                }
            }
        }

        private bool CanExecuteMoveSendToTargetDown(object? parameter)
        {
            if (parameter is SendToTarget target)
            {
                int index = SendToTargets.IndexOf(target);
                return index < SendToTargets.Count - 1 && index > -1;
            }
            return false;
        }

        private void ExecuteMoveSendToTargetDown(object? parameter)
        {
            if (parameter is SendToTarget target)
            {
                int index = SendToTargets.IndexOf(target);
                if (index < SendToTargets.Count - 1 && index > -1)
                {
                    SendToTargets.Move(index, index + 1);
                }
            }
        }

        private void ExecuteAddBlacklistedApp(string? appName)
        {
            if (string.IsNullOrWhiteSpace(appName) || !appName.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
            {
                _ = ShowInformationDialogAsync("Invalid Application Name", new System.Windows.Controls.TextBlock { Text = "Please enter a valid executable name ending with .exe." });
                return;
            }
            if (Settings.BlacklistedApps.Any(b => b.Equals(appName, StringComparison.OrdinalIgnoreCase)))
            {
                return; // Already exists
            }
            Settings.BlacklistedApps.Add(appName.Trim());
        }

        private void ExecuteRemoveBlacklistedApp(string? appName)
        {
            if (appName != null)
            {
                Settings.BlacklistedApps.Remove(appName);
            }
        }

        private void ExecuteBrowseAndAddBlacklistedApp()
        {
            var openFileDialog = new OpenFileDialog
            {
                Filter = "Executable files (*.exe)|*.exe|All files (*.*)|*.*",
                Title = "Select an Application to Blacklist"
            };

            if (openFileDialog.ShowDialog() == true)
            {
                var appName = Path.GetFileName(openFileDialog.FileName);
                ExecuteAddBlacklistedApp(appName);
            }
        }

    }
}