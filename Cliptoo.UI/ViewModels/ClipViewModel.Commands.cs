using System.Diagnostics;
using System.IO;
using System.Windows.Input;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
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
        public ICommand SendToCommand { get; }

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
            LogManager.Log($"Toggling pin for clip: ID={Id}, NewState={(IsPinned ? "Pinned" : "Unpinned")}.");
            await _clipDataService.TogglePinAsync(Id, IsPinned).ConfigureAwait(false);
            MainViewModel.HandleClipPinToggle(this);
        }

        private async Task DeleteAsync()
        {
            LogManager.Log($"Deleting clip: ID={_clip.Id}.");
            var fullClip = await GetFullClipAsync().ConfigureAwait(false);
            await _clipDataService.DeleteClipAsync(fullClip ?? _clip).ConfigureAwait(false);

            MainViewModel.HandleClipDeletion(this);
        }

    }
}