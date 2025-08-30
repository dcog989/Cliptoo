using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Input;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Native;
using Cliptoo.UI.Helpers;
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

            LogManager.LogDebug($"TRANSFORM_DIAG: Starting transform '{transformType}' for Clip ID {Id}.");

            await Controller.MoveClipToTopAsync(Id).ConfigureAwait(false);

            var fullClip = await GetFullClipAsync().ConfigureAwait(false);
            if (fullClip == null)
            {
                LogManager.LogDebug("TRANSFORM_DIAG: Aborted, full clip content is null.");
                return;
            }

            LogManager.LogDebug($"TRANSFORM_DIAG: Full clip content (first 100 chars): {fullClip.Content?.Substring(0, Math.Min(100, fullClip.Content.Length))}");

            string contentToTransform = (fullClip.ClipType == AppConstants.ClipTypes.Rtf
                ? RtfUtils.ToPlainText(fullClip.Content ?? string.Empty)
                : fullClip.Content) ?? string.Empty;

            LogManager.LogDebug($"TRANSFORM_DIAG: Content to transform (is RTF: {fullClip.ClipType == AppConstants.ClipTypes.Rtf}): {contentToTransform.Substring(0, Math.Min(100, contentToTransform.Length))}");

            var transformedContent = Controller.TransformText(contentToTransform, transformType);
            LogManager.LogDebug($"TRANSFORM_DIAG: Transformed content: '{transformedContent}' (Length: {transformedContent.Length})");


            var mainWindow = Application.Current.MainWindow;
            if (mainWindow != null)
            {
                mainWindow.Hide();
            }

            // Poll for focus change to ensure the paste command is not sent to Cliptoo itself.
            var stopwatch = Stopwatch.StartNew();
            while (stopwatch.ElapsedMilliseconds < 500) // 500ms timeout
            {
                if (ProcessUtils.GetForegroundWindowProcessName() != "Cliptoo.UI.exe")
                {
                    break;
                }
                await Task.Delay(20).ConfigureAwait(false);
            }
            stopwatch.Stop();
            LogManager.LogDebug($"TRANSFORM_DIAG: Waited {stopwatch.ElapsedMilliseconds}ms for focus change.");


            await _pastingService.PasteTextAsync(transformedContent).ConfigureAwait(false);
            await Controller.UpdatePasteCountAsync().ConfigureAwait(false);

            MainViewModel.RefreshClipList();
        }

        private async Task ExecutePasteAs(bool plainText)
        {
            Application.Current.MainWindow?.Hide();

            var clip = await GetFullClipAsync().ConfigureAwait(false);
            if (clip == null) return;

            await _pastingService.PasteClipAsync(clip, forcePlainText: plainText).ConfigureAwait(false);
            await Controller.UpdatePasteCountAsync().ConfigureAwait(false);
        }

        private async Task ExecuteOpen()
        {
            var fullClip = await GetFullClipAsync().ConfigureAwait(false);
            if (fullClip?.Content == null) return;
            try
            {
                var path = fullClip.Content.Trim();
                if (string.IsNullOrEmpty(path)) return;

                System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(path) { UseShellExecute = true });
            }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or ObjectDisposedException or FileNotFoundException)
            {
                LogManager.Log(ex, $"Failed to open path: {fullClip.Content}");
                _notificationService.Show("Error", $"Could not open path: {ex.Message}", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        private async Task ExecuteSendTo(SendToTarget? target)
        {
            if (target == null)
            {
                LogManager.LogDebug("SENDTO_DIAG: ExecuteSendTo called with null target.");
                return;
            }
            LogManager.LogDebug($"SENDTO_DIAG: ExecuteSendTo called for target: {target.Name} ({target.Path})");

            var clip = await GetFullClipAsync().ConfigureAwait(false);
            if (clip?.Content == null)
            {
                LogManager.LogDebug("SENDTO_DIAG: Clip content is null, aborting.");
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
                await File.WriteAllTextAsync(tempFilePath, clip.Content).ConfigureAwait(false);
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
                    args = string.Format(System.Globalization.CultureInfo.InvariantCulture, target.Arguments, contentPath);
                }
                Process.Start(new ProcessStartInfo(target.Path, args) { UseShellExecute = true });
            }
            catch (Exception ex) when (ex is IOException or System.ComponentModel.Win32Exception or UnauthorizedAccessException or PlatformNotSupportedException)
            {
                LogManager.Log(ex, $"Failed to send to path: {target.Path} with content {contentPath}");
                _notificationService.Show("Error", $"Could not send to '{target.Name}'.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        private async Task TogglePinAsync()
        {
            IsPinned = !IsPinned;
            await Controller.TogglePinAsync(Id, IsPinned).ConfigureAwait(false);
            MainViewModel.HandleClipPinToggle(this);
        }

        private async Task DeleteAsync()
        {
            await Controller.DeleteClipAsync(_clip).ConfigureAwait(false);
            MainViewModel.HandleClipDeletion(this);
        }
    }
}