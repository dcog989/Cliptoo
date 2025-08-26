using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Native;
using Cliptoo.Core.Native.Models;
using Cliptoo.Core.Services;
using Cliptoo.Core.Services.Models;

namespace Cliptoo.Core
{
    public record MaintenanceResult(
        int DbClipsCleaned,
        int ImageCachePruned,
        int FaviconCachePruned,
        int ReclassifiedClips,
        int TempFilesCleaned,
        int IconCachePruned,
        double DatabaseSizeChangeMb
    );

    public class CliptooController
    {
        public event Action? NewClipAdded;
        public event Action? HistoryCleared;
        public event Action? SettingsChanged;
        public event Action? CachesCleared;
        public event Action<string, string>? ProcessingFailed;


        private readonly System.Timers.Timer _cleanupTimer;
        private readonly ISettingsManager _settingsManager;
        private readonly IDbManager _dbManager;
        private readonly IContentProcessor _contentProcessor;
        private readonly IFileTypeClassifier _fileTypeClassifier;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;
        private readonly ITextTransformer _textTransformer;
        private readonly ICompareToolService _compareToolService;
        private readonly string _tempPath;
        private readonly string _imageCachePath;
        private DateTime _lastActivityTimestamp = DateTime.UtcNow;
        private bool _isInitialized;
        private readonly LruCache<int, Clip> _clipCache;
        private const int ClipCacheSize = 20;

        public bool IsUiInteractive { get; set; }

        public IClipboardMonitor ClipboardMonitor { get; }
        public Task<Clip> GetClipPreviewAsync(int id) => _dbManager.GetClipPreviewContentByIdAsync(id);

        private readonly IIconProvider _iconProvider;

        public CliptooController(
            ISettingsManager settingsManager,
            IDbManager dbManager,
            IContentProcessor contentProcessor,
            IFileTypeClassifier fileTypeClassifier,
            IThumbnailService thumbnailService,
            IWebMetadataService webMetadataService,
            IClipboardMonitor clipboardMonitor,
            ITextTransformer textTransformer,
            ICompareToolService compareToolService,
            IIconProvider iconProvider)
        {
            _settingsManager = settingsManager;
            _dbManager = dbManager;
            _contentProcessor = contentProcessor;
            _fileTypeClassifier = fileTypeClassifier;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
            ClipboardMonitor = clipboardMonitor;
            _textTransformer = textTransformer;
            _compareToolService = compareToolService;
            _iconProvider = iconProvider;
            _clipCache = new LruCache<int, Clip>(ClipCacheSize);

            _tempPath = Path.Combine(Path.GetTempPath(), "Cliptoo");
            Directory.CreateDirectory(_tempPath);

            var appDataLocalPath = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            _imageCachePath = Path.Combine(appDataLocalPath, "Cliptoo", "ImageCache");
            Directory.CreateDirectory(_imageCachePath);

            _cleanupTimer = new System.Timers.Timer();
            _cleanupTimer.Elapsed += OnCleanupTimerElapsed;
            _cleanupTimer.Interval = TimeSpan.FromHours(2).TotalMilliseconds;
        }

        public async Task InitializeAsync()
        {
            LogManager.Log("CliptooController initializing...");
            LogManager.LoggingLevel = GetSettings().LoggingLevel;
            CleanupTempFiles();
            await _dbManager.InitializeAsync().ConfigureAwait(false);
            LogManager.Log("Database initialized successfully.");

            ClipboardMonitor.ClipboardChanged += OnClipboardChangedAsync;
            _cleanupTimer.Start();
            LogManager.Log("CliptooController initialization complete.");
            _isInitialized = true;
            _fileTypeClassifier.FileTypesChanged += OnFileTypesChanged;
        }

        private void ExecuteSafely(Func<Task> action, string context)
        {
            Task.Run(async () =>
            {
                try
                {
                    await action().ConfigureAwait(false);
                }
                catch (Exception ex)
                {
                    LogManager.Log(ex, $"Error in {context}. The operation will be skipped, but the application will continue.");
                    if (context == nameof(OnClipboardChangedAsync))
                    {
                        ProcessingFailed?.Invoke("Failed to Save Clip", "Could not process and save the latest clipboard item. See logs for details.");
                    }
                }
            });
        }

        private (string, bool) TruncateText(string text, long maxBytes, string logContext)
        {
            if (Encoding.UTF8.GetByteCount(text) <= maxBytes)
            {
                return (text, false);
            }

            var encoder = Encoding.UTF8.GetEncoder();
            var bytes = new byte[maxBytes];
            encoder.Convert(text.AsSpan(), bytes, true, out int charsUsed, out _, out _);
            var truncatedText = text.Substring(0, charsUsed);
            LogManager.Log($"{logContext} truncated to {maxBytes} bytes.");
            return (truncatedText, true);
        }

        private void OnClipboardChangedAsync(object? sender, ClipboardChangedEventArgs e)
        {
            ExecuteSafely(async () =>
            {
                var stopwatch = System.Diagnostics.Stopwatch.StartNew();
                ProcessingResult? result = null;
                var settings = GetSettings();
                bool wasTruncated = false;
                long maxBytes = (long)settings.MaxClipSizeMb * 1024 * 1024;

                if (e.ContentType == ClipboardContentType.Text)
                {
                    var textContent = (string)e.Content;
                    (textContent, wasTruncated) = TruncateText(textContent, maxBytes, "Text clip");

                    if (e.IsRtf)
                    {
                        result = new ProcessingResult(AppConstants.ClipTypes.Rtf, textContent);
                    }
                    else
                    {
                        result = _contentProcessor.Process(textContent);
                    }
                }
                else if (e.ContentType == ClipboardContentType.Image)
                {
                    var imageBytes = (byte[])e.Content;
                    if (imageBytes.Length > maxBytes)
                    {
                        LogManager.Log($"Image clip of size {imageBytes.Length} bytes exceeds limit of {maxBytes} bytes. Clip will be ignored.");
                        return;
                    }

                    var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(imageBytes)).ToLowerInvariant();
                    var permanentImagePath = Path.Combine(_imageCachePath, $"{hash}.png");

                    if (!File.Exists(permanentImagePath))
                    {
                        await File.WriteAllBytesAsync(permanentImagePath, imageBytes).ConfigureAwait(false);
                    }

                    await _thumbnailService.GetThumbnailAsync(permanentImagePath, null).ConfigureAwait(false);
                    result = new ProcessingResult(AppConstants.ClipTypes.Image, permanentImagePath);
                }

                if (result != null)
                {
                    bool finalWasTrimmed = result.WasTrimmed || wasTruncated;
                    string? sourceApp = result.SourceAppOverride ?? e.SourceApp;
                    int newClipId = await _dbManager.AddClipAsync(result.Content, result.ClipType, sourceApp, finalWasTrimmed).ConfigureAwait(false);
                    NewClipAdded?.Invoke();
                    NotifyUiActivity();
                }
                stopwatch.Stop();
                LogManager.LogDebug($"PERF_DIAG: OnClipboardChangedAsync processed in {stopwatch.ElapsedMilliseconds}ms.");
            }, nameof(OnClipboardChangedAsync));
        }

        public Task<List<Clip>> GetClipsAsync(uint limit = 100, uint offset = 0, string searchTerm = "", string filterType = "all", CancellationToken cancellationToken = default, [System.Runtime.CompilerServices.CallerMemberName] string caller = "")
        {
            LogManager.Log($"SEARCH_DIAG: Controller.GetClipsAsync called from '{caller}'.");
            return _dbManager.GetClipsAsync(limit, offset, searchTerm, filterType, cancellationToken);
        }

        public async Task<Clip> GetClipByIdAsync(int id)
        {
            if (_clipCache.TryGetValue(id, out var cachedClip) && cachedClip is not null)
            {
                return cachedClip;
            }

            var clip = await _dbManager.GetClipByIdAsync(id).ConfigureAwait(false);

            if (clip.SizeInBytes < 100 * 1024) // Only cache clips < 100 KB
            {
                _clipCache.Add(id, clip);
            }

            return clip;
        }

        public async Task DeleteClipAsync(Clip clip)
        {
            await _dbManager.DeleteClipAsync(clip.Id).ConfigureAwait(false);
            _clipCache.Remove(clip.Id);

            if (clip.ClipType == AppConstants.ClipTypes.Link && clip.Content is not null)
            {
                _webMetadataService.ClearCacheForUrl(clip.Content);
            }
        }

        public Task TogglePinAsync(int id, bool isPinned)
        {
            return _dbManager.TogglePinAsync(id, isPinned);
        }

        public async Task UpdateClipContentAsync(int id, string newContent)
        {
            await _dbManager.UpdateClipContentAsync(id, newContent).ConfigureAwait(false);
            _clipCache.Remove(id);
        }

        public Task UpdatePasteCountAsync()
        {
            return _dbManager.UpdatePasteCountAsync();
        }

        public async Task MoveClipToTopAsync(int id)
        {
            await _dbManager.UpdateTimestampAsync(id).ConfigureAwait(false);
            _clipCache.Remove(id);
        }

        public async Task<string> GetTransformedContentAsync(int id, string transformType)
        {
            var clip = await GetClipByIdAsync(id).ConfigureAwait(false);
            if (clip?.Content == null) return string.Empty;

            if (clip.ClipType != AppConstants.ClipTypes.Text && !clip.ClipType.StartsWith("code"))
            {
                return clip.Content;
            }

            return _textTransformer.Transform(clip.Content, transformType);
        }

        public async Task<(bool success, string message)> CompareClipsAsync(int leftClipId, int rightClipId)
        {
            string? toolPath = GetSettings().CompareToolPath;
            string? toolArgs;

            if (string.IsNullOrWhiteSpace(toolPath) || !File.Exists(toolPath))
            {
                (toolPath, toolArgs) = _compareToolService.FindCompareTool();
            }
            else
            {
                toolArgs = _compareToolService.GetArgsForPath(toolPath);
            }

            if (string.IsNullOrEmpty(toolPath))
            {
                return (false, "No supported text comparison tool found. Configure one in Settings or install a supported tool.");
            }

            try
            {
                var leftClip = await GetClipByIdAsync(leftClipId).ConfigureAwait(false);
                var rightClip = await GetClipByIdAsync(rightClipId).ConfigureAwait(false);

                var leftFilePath = Path.Combine(_tempPath, $"cliptoo_compare_left_{Guid.NewGuid()}.txt");
                var rightFilePath = Path.Combine(_tempPath, $"cliptoo_compare_right_{Guid.NewGuid()}.txt");

                await File.WriteAllTextAsync(leftFilePath, leftClip.Content ?? "").ConfigureAwait(false);
                await File.WriteAllTextAsync(rightFilePath, rightClip.Content ?? "").ConfigureAwait(false);

                var process = new System.Diagnostics.Process();
                process.StartInfo.FileName = toolPath;
                process.StartInfo.Arguments = $"{toolArgs} \"{leftFilePath}\" \"{rightFilePath}\"";
                process.StartInfo.UseShellExecute = true;
                process.Start();

                return (true, "Comparison tool launched.");
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, "Failed to execute clip comparison.");
                return (false, "An error occurred while launching the comparison tool.");
            }
        }

        public Settings GetSettings() => _settingsManager.Load();

        public void SaveSettings(Settings settings)
        {
            _settingsManager.Save(settings);
            LogManager.LoggingLevel = settings.LoggingLevel;
            SettingsChanged?.Invoke();
        }

        public Task<DbStats> GetStatsAsync() => _dbManager.GetStatsAsync();

        public async Task ClearHistoryAsync()
        {
            await _dbManager.ClearHistoryAsync().ConfigureAwait(false);
            HistoryCleared?.Invoke();
        }

        public async Task ClearAllHistoryAsync()
        {
            await _dbManager.ClearAllHistoryAsync().ConfigureAwait(false);
            HistoryCleared?.Invoke();
        }

        public async Task<MaintenanceResult> RunHeavyMaintenanceNowAsync()
        {
            LogManager.Log("User triggered heavy maintenance routine.");
            return await RunHeavyMaintenanceAsync().ConfigureAwait(false);
        }

        public async Task<int> RemoveDeadheadClipsAsync()
        {
            int count = await _dbManager.RemoveDeadheadClipsAsync().ConfigureAwait(false);
            if (count > 0)
            {
                HistoryCleared?.Invoke();
            }
            return count;
        }

        public async Task<int> ClearOversizedClipsAsync(uint sizeMb)
        {
            int count = await _dbManager.ClearOversizedClipsAsync(sizeMb).ConfigureAwait(false);
            if (count > 0)
            {
                HistoryCleared?.Invoke();
            }
            return count;
        }

        public async Task<int> ReclassifyAllClipsAsync()
        {
            var fileClips = await _dbManager.GetAllFileBasedClipsAsync().ConfigureAwait(false);
            var updates = new Dictionary<int, string>();

            foreach (var clip in fileClips)
            {
                var newClipType = _fileTypeClassifier.Classify(clip.Content ?? "");
                if (clip.ClipType != newClipType)
                {
                    updates[clip.Id] = newClipType;
                }
            }

            if (updates.Any())
            {
                await _dbManager.UpdateClipTypesAsync(updates).ConfigureAwait(false);
                HistoryCleared?.Invoke();
            }

            return updates.Count;
        }

        private void OnFileTypesChanged()
        {
            if (!_isInitialized)
            {
                LogManager.LogDebug("OnFileTypesChanged skipped during initialization.");
                return;
            }

            ExecuteSafely(async () =>
            {
                LogManager.Log("File type definitions changed, starting re-classification of existing clips.");
                int count = await ReclassifyAllClipsAsync().ConfigureAwait(false);
                LogManager.Log($"Re-classification complete. {count} clips updated.");
            }, nameof(OnFileTypesChanged));
        }

        private async Task<MaintenanceResult> RunHeavyMaintenanceAsync()
        {
            LogManager.Log("Starting heavy maintenance routine...");
            var settings = GetSettings();
            var initialStats = await _dbManager.GetStatsAsync().ConfigureAwait(false);

            int tempFilesCleaned = CleanupTempFiles();

            int cleaned = await _dbManager.PerformCleanupAsync(settings.CleanupAgeDays, settings.MaxClipsTotal, true).ConfigureAwait(false);
            if (cleaned > 0)
            {
                LogManager.Log($"Database cleanup complete. Removed {cleaned} items.");
            }

            var validImagePathsStream = _dbManager.GetAllImageClipPathsAsync();
            int prunedImageCount = await _thumbnailService.PruneCacheAsync(validImagePathsStream, settings.HoverImagePreviewSize).ConfigureAwait(false);
            if (prunedImageCount > 0)
            {
                LogManager.Log($"Image cache cleanup complete. Removed {prunedImageCount} orphaned files.");
            }

            var validLinkUrlsStream = _dbManager.GetAllLinkClipUrlsAsync();
            int prunedFaviconCount = await _webMetadataService.PruneCacheAsync(validLinkUrlsStream).ConfigureAwait(false);
            if (prunedFaviconCount > 0)
            {
                LogManager.Log($"Favicon cache cleanup complete. Removed {prunedFaviconCount} orphaned files.");
            }

            int iconCacheCleaned = _iconProvider.CleanupIconCache();

            int reclassifiedCount = await ReclassifyAllClipsAsync().ConfigureAwait(false);
            if (reclassifiedCount > 0)
            {
                LogManager.Log($"File re-classification complete. Updated {reclassifiedCount} clips.");
            }

            await _dbManager.UpdateLastCleanupTimestampAsync().ConfigureAwait(false);

            var finalStats = await _dbManager.GetStatsAsync().ConfigureAwait(false);
            double sizeChange = Math.Round(initialStats.DatabaseSizeMb - finalStats.DatabaseSizeMb, 2);

            LogManager.Log("Heavy maintenance routine finished.");

            return new MaintenanceResult(
                cleaned,
                prunedImageCount,
                prunedFaviconCount,
                reclassifiedCount,
                tempFilesCleaned,
                iconCacheCleaned,
                sizeChange
            );
        }


        private void OnCleanupTimerElapsed(object? sender, System.Timers.ElapsedEventArgs e)
        {
            ExecuteSafely(async () =>
            {
                if (IsUiInteractive || (DateTime.UtcNow - _lastActivityTimestamp) < TimeSpan.FromMinutes(5))
                {
                    LogManager.LogDebug("Cleanup skipped due to recent activity or visible UI.");
                    return;
                }

                var stats = await _dbManager.GetStatsAsync().ConfigureAwait(false);
                var lastCleanup = stats.LastCleanupTimestamp ?? DateTime.MinValue;

                if ((DateTime.UtcNow - lastCleanup) > TimeSpan.FromDays(1))
                {
                    LogManager.Log("Idle timer check: Last heavy maintenance was over 24 hours ago. Triggering routine.");
                    await RunHeavyMaintenanceAsync().ConfigureAwait(false);
                }
                else
                {
                    LogManager.LogDebug("Idle timer check: Heavy maintenance not due yet.");
                }

            }, nameof(OnCleanupTimerElapsed));
        }

        public void ClearCaches()
        {
            LogManager.Log("Clearing thumbnail and temp file caches.");
            _thumbnailService.ClearCache();
            _webMetadataService.ClearCache();
            CleanupTempFiles();
            CachesCleared?.Invoke();
        }

        private int CleanupTempFiles()
        {
            try
            {
                var oldPngFiles = Directory.EnumerateFiles(_tempPath, "cliptoo_*.png")
                    .Where(f => (DateTime.UtcNow - new FileInfo(f).CreationTimeUtc) > TimeSpan.FromHours(1));

                var oldTxtFiles = Directory.EnumerateFiles(_tempPath, "cliptoo_compare_*.txt")
                    .Where(f => (DateTime.UtcNow - new FileInfo(f).CreationTimeUtc) > TimeSpan.FromHours(1));

                var oldFiles = oldPngFiles.Concat(oldTxtFiles);

                int filesDeleted = 0;
                foreach (var file in oldFiles)
                {
                    try
                    {
                        File.Delete(file);
                        filesDeleted++;
                    }
                    catch (Exception ex)
                    {
                        LogManager.Log(ex, $"Could not delete old temp file: {file}");
                    }
                }
                if (filesDeleted > 0)
                {
                    LogManager.Log($"Cleaned up {filesDeleted} old temporary files.");
                }
                return filesDeleted;
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, "Failed to perform temp file cleanup.");
                return 0;
            }
        }

        public void Shutdown()
        {
            LogManager.Log("Cliptoo shutting down.");
            _cleanupTimer.Stop();
            _cleanupTimer.Dispose();
            _fileTypeClassifier.FileTypesChanged -= OnFileTypesChanged;
            ClipboardMonitor.Dispose();
            _dbManager.Dispose();
        }

        public bool IsCompareToolAvailable()
        {
            string? toolPath = GetSettings().CompareToolPath;
            if (!string.IsNullOrWhiteSpace(toolPath) && File.Exists(toolPath))
            {
                return true;
            }

            (toolPath, _) = _compareToolService.FindCompareTool();
            return !string.IsNullOrEmpty(toolPath);
        }

        public void NotifyUiActivity()
        {
            _lastActivityTimestamp = DateTime.UtcNow;
        }

        public void SuppressNextClip(ulong hash)
        {
            ClipboardMonitor.SuppressNextClip(hash);
        }
    }
}