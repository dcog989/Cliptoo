using System;
using System.IO;
using System.Threading.Tasks;
using Cliptoo.Core.Database;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Services
{
    public class ClipboardService : IClipboardService
    {
        private readonly IDbManager _dbManager;
        private readonly IClipDataService _clipDataService;
        private readonly ITextTransformer _textTransformer;
        private readonly ICompareToolService _compareToolService;
        private readonly ISettingsService _settingsService;
        private readonly string _tempPath;

        public ClipboardService(
            IDbManager dbManager,
            IClipDataService clipDataService,
            ITextTransformer textTransformer,
            ICompareToolService compareToolService,
            ISettingsService settingsService)
        {
            _dbManager = dbManager;
            _clipDataService = clipDataService;
            _textTransformer = textTransformer;
            _compareToolService = compareToolService;
            _settingsService = settingsService;
            _tempPath = Path.Combine(Path.GetTempPath(), "Cliptoo");
            Directory.CreateDirectory(_tempPath);
        }

        public Task UpdatePasteCountAsync()
        {
            return _dbManager.UpdatePasteCountAsync();
        }

        public string TransformText(string content, string transformType)
        {
            return _textTransformer.Transform(content, transformType);
        }

        private (string? Path, string? Args) GetCompareTool()
        {
            string? toolPath = _settingsService.Settings.CompareToolPath;
            string? toolArgs;

            if (string.IsNullOrWhiteSpace(toolPath) || !File.Exists(toolPath))
            {
                (toolPath, toolArgs) = _compareToolService.FindCompareTool();
            }
            else
            {
                toolArgs = _compareToolService.GetArgsForPath(toolPath);
            }
            return (toolPath, toolArgs);
        }

        public async Task<(bool success, string message)> CompareClipsAsync(int leftClipId, int rightClipId)
        {
            var (toolPath, toolArgs) = GetCompareTool();

            if (string.IsNullOrEmpty(toolPath))
            {
                return (false, "No supported text comparison tool found. Configure one in Settings or install a supported tool.");
            }

            string? leftFilePath = null;
            string? rightFilePath = null;
            try
            {
                var leftClip = await _clipDataService.GetClipByIdAsync(leftClipId).ConfigureAwait(false);
                var rightClip = await _clipDataService.GetClipByIdAsync(rightClipId).ConfigureAwait(false);

                if (leftClip is null || rightClip is null)
                {
                    return (false, "One or both of the clips to compare could not be found.");
                }

                leftFilePath = Path.Combine(_tempPath, $"cliptoo_compare_left_{Guid.NewGuid()}.txt");
                rightFilePath = Path.Combine(_tempPath, $"cliptoo_compare_right_{Guid.NewGuid()}.txt");

                await File.WriteAllTextAsync(leftFilePath, leftClip.Content ?? "").ConfigureAwait(false);
                await File.WriteAllTextAsync(rightFilePath, rightClip.Content ?? "").ConfigureAwait(false);

                using (var process = new System.Diagnostics.Process())
                {
                    process.StartInfo.FileName = toolPath;
                    process.StartInfo.Arguments = $"{toolArgs} \"{leftFilePath}\" \"{rightFilePath}\"";
                    process.StartInfo.UseShellExecute = false;
                    process.Start();
                    await process.WaitForExitAsync().ConfigureAwait(false);
                }

                return (true, "Comparison tool launched.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.ComponentModel.Win32Exception or ObjectDisposedException or FileNotFoundException)
            {
                LogManager.LogCritical(ex, "Failed to execute clip comparison.");
                return (false, "An error occurred while launching the comparison tool.");
            }
            finally
            {
                try
                {
                    if (leftFilePath != null && File.Exists(leftFilePath))
                    {
                        File.Delete(leftFilePath);
                    }
                    if (rightFilePath != null && File.Exists(rightFilePath))
                    {
                        File.Delete(rightFilePath);
                    }
                }
                catch (IOException ex)
                {
                    LogManager.LogWarning($"Failed to delete temporary compare files: {ex.Message}");
                }
            }
        }

        public bool IsCompareToolAvailable()
        {
            var (toolPath, _) = GetCompareTool();
            return !string.IsNullOrEmpty(toolPath);
        }
    }
}