using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Input;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    public partial class ClipViewModel
    {
        public ICommand TogglePinCommand { get; }
        public ICommand DeleteCommand { get; }
        public ICommand EditClipCommand { get; }
        public ICommand MoveToTopCommand { get; }
        public ICommand OpenCommand { get; }
        public ICommand SelectForCompareLeftCommand { get; }
        public ICommand CompareWithSelectedRightCommand { get; }
        public ICommand PasteAsPlainTextCommand { get; }
        public ICommand PasteAsRtfCommand { get; }
        public ICommand TransformAndPasteCommand { get; }
        public ICommand SendToCommand { get; }

        private async Task ExecuteTransformAndPaste(string? transformType)
        {
            if (transformType == null) return;

            await Controller.MoveClipToTopAsync(Id);
            MainViewModel.RefreshClipList();

            var transformedContent = await Controller.GetTransformedContentAsync(Id, transformType);
            if (!string.IsNullOrEmpty(transformedContent))
            {
                await _pastingService.PasteTextAsync(transformedContent);
                await Controller.UpdatePasteCountAsync();
            }

            var mainWindow = Application.Current.MainWindow;
            if (mainWindow != null)
            {
                mainWindow.Hide();
            }
        }

        private async Task ExecutePasteAs(bool plainText)
        {
            Application.Current.MainWindow?.Hide();

            var clip = await GetFullClipAsync();
            if (clip == null) return;

            await _pastingService.PasteClipAsync(clip, forcePlainText: plainText);
            await Controller.UpdatePasteCountAsync();
        }

        private async Task ExecuteOpen()
        {
            var fullClip = await GetFullClipAsync();
            if (fullClip?.Content == null) return;
            try
            {
                var path = fullClip.Content.Trim();
                if (string.IsNullOrEmpty(path)) return;

                System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(path) { UseShellExecute = true });
            }
            catch (Exception ex)
            {
                Core.Configuration.LogManager.Log(ex, $"Failed to open path: {fullClip.Content}");
                _notificationService.Show("Error", $"Could not open path: {ex.Message}", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        private async Task ExecuteSendTo(SendToTarget? target)
        {
            if (target == null)
            {
                Core.Configuration.LogManager.LogDebug("SENDTO_DIAG: ExecuteSendTo called with null target.");
                return;
            }
            Core.Configuration.LogManager.LogDebug($"SENDTO_DIAG: ExecuteSendTo called for target: {target.Name} ({target.Path})");

            var clip = await GetFullClipAsync();
            if (clip?.Content == null)
            {
                Core.Configuration.LogManager.LogDebug("SENDTO_DIAG: Clip content is null, aborting.");
                return;
            }

            string contentPath;

            if (IsFileBased)
            {
                contentPath = clip.Content.Trim();
            }
            else
            {
                var extension = ClipType switch
                {
                    AppConstants.ClipTypes.CodeSnippet => ".txt",
                    AppConstants.ClipTypes.Rtf => ".rtf",
                    _ => ".txt"
                };
                var tempFilePath = Path.Combine(Path.GetTempPath(), $"cliptoo_sendto_{Guid.NewGuid()}{extension}");
                await File.WriteAllTextAsync(tempFilePath, clip.Content);
                contentPath = tempFilePath;
            }

            try
            {
                string args;
                if (string.IsNullOrWhiteSpace(target.Arguments))
                {
                    args = $"\"{contentPath}\"";
                }
                else
                {
                    args = string.Format(target.Arguments, contentPath);
                }
                Process.Start(new ProcessStartInfo(target.Path, args) { UseShellExecute = true });
            }
            catch (Exception ex)
            {
                Core.Configuration.LogManager.Log(ex, $"Failed to send to path: {target.Path} with content {contentPath}");
                _notificationService.Show("Error", $"Could not send to '{target.Name}'.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        private async Task TogglePinAsync()
        {
            IsPinned = !IsPinned;
            await Controller.TogglePinAsync(Id, IsPinned);
            MainViewModel.HandleClipPinToggle(this);
        }

        private async Task DeleteAsync()
        {
            await Controller.DeleteClipAsync(_clip);
            MainViewModel.HandleClipDeletion(this);
        }
    }
}