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
            _dbManager = dbManager ?? throw new ArgumentNullException(nameof(dbManager));
            _clipDataService = clipDataService ?? throw new ArgumentNullException(nameof(clipDataService));
            _textTransformer = textTransformer ?? throw new ArgumentNullException(nameof(textTransformer));
            _compareToolService = compareToolService ?? throw new ArgumentNullException(nameof(compareToolService));
            _settingsService = settingsService ?? throw new ArgumentNullException(nameof(settingsService));
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

                await File.WriteAllTextAsync(leftFilePath, leftClip.Content ?? string.Empty).ConfigureAwait(false);
                await File.WriteAllTextAsync(rightFilePath, rightClip.Content ?? string.Empty).ConfigureAwait(false);

                using (var process = new System.Diagnostics.Process())
                {
                    process.StartInfo.FileName = toolPath;
                    process.StartInfo.Arguments = string.IsNullOrEmpty(toolArgs)
                        ? $"\"{leftFilePath}\" \"{rightFilePath}\""
                        : $"{toolArgs} \"{leftFilePath}\" \"{rightFilePath}\"";
                    process.StartInfo.UseShellExecute = false;
                    process.StartInfo.CreateNoWindow = false;

                    if (!process.Start())
                    {
                        return (false, "Failed to start the comparison tool.");
                    }

                    // Don't wait for the process to exit - let it run independently
                    // The comparison tool should stay open for the user to review
                }

                // Schedule cleanup after a delay to ensure the comparison tool has loaded the files
                _ = Task.Run(async () =>
                {
                    await Task.Delay(5000).ConfigureAwait(false); // Wait 5 seconds
                    CleanupTempFiles(leftFilePath, rightFilePath);
                });

                return (true, "Comparison tool launched successfully.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.ComponentModel.Win32Exception or FileNotFoundException or InvalidOperationException)
            {
                LogManager.LogCritical(ex, "Failed to execute clip comparison.");

                // Attempt immediate cleanup on error
                CleanupTempFiles(leftFilePath, rightFilePath);

                return (false, $"An error occurred while launching the comparison tool: {ex.Message}");
            }
        }

        static void CleanupTempFiles(string? leftFilePath, string? rightFilePath)
        {
            try
            {
                if (leftFilePath != null && File.Exists(leftFilePath))
                {
                    File.Delete(leftFilePath);
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.LogWarning($"Failed to delete temporary file '{leftFilePath}': {ex.Message}");
            }

            try
            {
                if (rightFilePath != null && File.Exists(rightFilePath))
                {
                    File.Delete(rightFilePath);
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.LogWarning($"Failed to delete temporary file '{rightFilePath}': {ex.Message}");
            }
        }

        public bool IsCompareToolAvailable()
        {
            var (toolPath, _) = GetCompareTool();
            return !string.IsNullOrEmpty(toolPath);
        }
    }
}
