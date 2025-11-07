using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Services
{
    public class DatabaseService : IDatabaseService, IDisposable
    {
        private readonly IDbManager _dbManager;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;
        private readonly IIconCacheManager _iconCacheManager;
        private readonly IFileTypeClassifier _fileTypeClassifier;
        private readonly ISettingsService _settingsService;
        private readonly string _clipboardImageCachePath;
        private readonly string _tempPath;
        private readonly SemaphoreSlim _maintenanceLock = new(1, 1);
        private bool _disposedValue;

        private static readonly JsonSerializerOptions _exportJsonOptions = new()
        {
            WriteIndented = true,
            Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping
        };

        private static readonly JsonSerializerOptions _importJsonOptions = new()
        {
            PropertyNameCaseInsensitive = true,
        };

        public event EventHandler? HistoryCleared;

        public DatabaseService(
            IDbManager dbManager,
            IThumbnailService thumbnailService,
            IWebMetadataService webMetadataService,
            IIconCacheManager iconCacheManager,
            IFileTypeClassifier fileTypeClassifier,
            ISettingsService settingsService,
            string appDataLocalPath)
        {
            _dbManager = dbManager;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
            _iconCacheManager = iconCacheManager;
            _fileTypeClassifier = fileTypeClassifier;
            _settingsService = settingsService;

            _clipboardImageCachePath = Path.Combine(appDataLocalPath, "Cliptoo", "ClipboardImageCache");
            _tempPath = Path.Combine(Path.GetTempPath(), "Cliptoo");
        }

        public Task<DbStats> GetStatsAsync() => _dbManager.GetStatsAsync();

        public async Task ClearHistoryAsync()
        {
            await _dbManager.ClearHistoryAsync().ConfigureAwait(false);
            HistoryCleared?.Invoke(this, EventArgs.Empty);
        }

        public async Task ClearAllHistoryAsync()
        {
            await _dbManager.ClearAllHistoryAsync().ConfigureAwait(false);
            HistoryCleared?.Invoke(this, EventArgs.Empty);
        }

        public async Task ClearFavoriteClipsAsync()
        {
            await _dbManager.ClearFavoriteClipsAsync().ConfigureAwait(false);
            HistoryCleared?.Invoke(this, EventArgs.Empty);
        }

        public void ClearCaches()
        {
            LogManager.LogInfo("Clearing thumbnail and temp file caches.");
            _thumbnailService.ClearCache();
            _webMetadataService.ClearCache();
            _iconCacheManager.ClearCache();
            ServiceUtils.DeleteDirectoryContents(_clipboardImageCachePath);
            CleanupTempFiles();
        }

        public async Task<MaintenanceResult> RunHeavyMaintenanceNowAsync()
        {
            if (_maintenanceLock.CurrentCount == 0)
            {
                LogManager.LogDebug("Maintenance task skipped because another one is already in progress.");
                return new MaintenanceResult(0, 0, 0, 0, 0, 0, 0, 0.0);
            }

            await _maintenanceLock.WaitAsync().ConfigureAwait(false);
            try
            {
                LogManager.LogInfo("User or schedule triggered heavy maintenance routine.");
                return await RunHeavyMaintenanceAsync().ConfigureAwait(false);
            }
            finally
            {
                _maintenanceLock.Release();
            }
        }

        public async Task<int> RemoveDeadheadClipsAsync()
        {
            var count = await _dbManager.RemoveDeadheadClipsAsync().ConfigureAwait(false);
            if (count > 0)
            {
                HistoryCleared?.Invoke(this, EventArgs.Empty);
            }
            return count;
        }

        public async Task<int> ClearOversizedClipsAsync(uint sizeMb)
        {
            var count = await _dbManager.ClearOversizedClipsAsync(sizeMb).ConfigureAwait(false);
            if (count > 0)
            {
                HistoryCleared?.Invoke(this, EventArgs.Empty);
            }
            return count;
        }

        public async Task<int> ReclassifyAllClipsAsync()
        {
            var updates = new Dictionary<int, string>();
            var enumerator = _dbManager.GetAllFileBasedClipsAsync().GetAsyncEnumerator();
            try
            {
                while (await enumerator.MoveNextAsync().ConfigureAwait(false))
                {
                    var clip = enumerator.Current;
                    var newClipType = _fileTypeClassifier.Classify(clip.Content ?? "");
                    if (clip.ClipType != newClipType)
                    {
                        updates[clip.Id] = newClipType;
                    }
                }
            }
            finally
            {
                await enumerator.DisposeAsync().ConfigureAwait(false);
            }

            if (updates.Count > 0)
            {
                await _dbManager.UpdateClipTypesAsync(updates).ConfigureAwait(false);
                HistoryCleared?.Invoke(this, EventArgs.Empty);
            }

            return updates.Count;
        }

        public async Task<string> ExportToJsonStringAsync(bool favoriteOnly)
        {
            var clips = new List<Clip>();
            await foreach (var clip in _dbManager.GetAllClipsAsync(favoriteOnly).ConfigureAwait(false))
            {
                clips.Add(clip);
            }

            return JsonSerializer.Serialize(clips, _exportJsonOptions);
        }

        public async Task<int> ImportFromJsonAsync(string jsonContent)
        {
            try
            {
                var clips = JsonSerializer.Deserialize<List<Clip>>(jsonContent, _importJsonOptions);

                if (clips == null || clips.Count == 0)
                {
                    return 0;
                }

                var importedCount = await _dbManager.AddClipsAsync(clips).ConfigureAwait(false);
                if (importedCount > 0)
                {
                    await _dbManager.CompactDbAsync().ConfigureAwait(false);
                    HistoryCleared?.Invoke(this, EventArgs.Empty);
                }
                return importedCount;
            }
            catch (JsonException ex)
            {
                LogManager.LogCritical(ex, "Failed to deserialize JSON during import.");
                throw; // Re-throw to be caught by the ViewModel
            }
        }

        public void ClearLogs()
        {
            LogManager.ClearLogs();
        }

        private async Task<MaintenanceResult> RunHeavyMaintenanceAsync()
        {
            LogManager.LogInfo("Starting heavy maintenance routine...");
            var settings = _settingsService.Settings;
            var initialStats = await _dbManager.GetStatsAsync().ConfigureAwait(false);

            int tempFilesCleaned = CleanupTempFiles();

            int cleaned = await _dbManager.PerformCleanupAsync(settings.CleanupAgeDays, settings.MaxClipsTotal, true).ConfigureAwait(false);
            if (cleaned > 0)
            {
                LogManager.LogInfo($"Database cleanup complete. Removed {cleaned} items.");
            }

            var validImagePathsStream = _dbManager.GetAllImageClipPathsAsync();
            int prunedImageCount = await _thumbnailService.PruneCacheAsync(validImagePathsStream, settings.HoverImagePreviewSize).ConfigureAwait(false);
            if (prunedImageCount > 0)
            {
                LogManager.LogInfo($"Image cache cleanup complete. Removed {prunedImageCount} orphaned files.");
            }

            int prunedFaviconCount = await _webMetadataService.PruneCacheAsync().ConfigureAwait(false);
            if (prunedFaviconCount > 0)
            {
                LogManager.LogInfo($"Favicon cache cleanup complete. Removed {prunedFaviconCount} orphaned files.");
            }

            int iconCacheCleaned = _iconCacheManager.CleanupIconCache();

            var validClipboardImages = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            await foreach (var path in _dbManager.GetAllImageClipPathsAsync().ConfigureAwait(false))
            {
                validClipboardImages.Add(path);
            }
            int prunedClipboardImageCount = await ServiceUtils.PruneDirectoryAsync(_clipboardImageCachePath, validClipboardImages).ConfigureAwait(false);
            if (prunedClipboardImageCount > 0)
            {
                LogManager.LogInfo($"Clipboard Image Cache cleanup complete. Removed {prunedClipboardImageCount} orphaned files.");
            }

            int reclassifiedCount = await ReclassifyAllClipsAsync().ConfigureAwait(false);
            if (reclassifiedCount > 0)
            {
                LogManager.LogInfo($"File re-classification complete. Updated {reclassifiedCount} clips.");
            }

            await _dbManager.UpdateLastCleanupTimestampAsync().ConfigureAwait(false);

            var finalStats = await _dbManager.GetStatsAsync().ConfigureAwait(false);
            double sizeChange = Math.Round(initialStats.DatabaseSizeMb - finalStats.DatabaseSizeMb, 2);

            var summaryParts = new List<string>();
            if (cleaned > 0) summaryParts.Add($"Removed {cleaned} clips");
            if (prunedImageCount > 0) summaryParts.Add($"pruned {prunedImageCount} images");
            if (prunedFaviconCount > 0) summaryParts.Add($"pruned {prunedFaviconCount} favicons");
            if (iconCacheCleaned > 0) summaryParts.Add($"pruned {iconCacheCleaned} icons");
            if (prunedClipboardImageCount > 0) summaryParts.Add($"pruned {prunedClipboardImageCount} clipboard images");
            if (reclassifiedCount > 0) summaryParts.Add($"reclassified {reclassifiedCount} files");
            if (tempFilesCleaned > 0) summaryParts.Add($"cleaned {tempFilesCleaned} temp files");

            string summary;
            if (summaryParts.Count > 0)
            {
                summary = "Maintenance complete. " + string.Join(", ", summaryParts) + ".";
            }
            else
            {
                summary = "Maintenance complete. No items required cleaning.";
            }
            LogManager.LogInfo(summary);

            LogManager.LogDebug("Heavy maintenance routine finished.");
            LogManager.LogDebug($"Maintenance result: {cleaned} clips cleaned, {prunedImageCount} images pruned, {prunedFaviconCount} favicons pruned, {reclassifiedCount} reclassified, {tempFilesCleaned} temp files cleaned, {iconCacheCleaned} icons pruned, {prunedClipboardImageCount} clipboard images pruned. DB size change: {sizeChange:F2} MB.");

            return new MaintenanceResult(
                            cleaned,
                prunedImageCount,
                prunedFaviconCount,
                reclassifiedCount,
                tempFilesCleaned,
                iconCacheCleaned,
                prunedClipboardImageCount,
                sizeChange
            );
        }

        public int CleanupTempFiles()
        {
            try
            {
                if (!Directory.Exists(_tempPath))
                {
                    return 0;
                }

                var oldFiles = Directory.EnumerateFiles(_tempPath, "cliptoo_*.*")
                    .Where(f => (DateTime.UtcNow - new FileInfo(f).CreationTimeUtc) > TimeSpan.FromHours(1));

                int filesDeleted = 0;
                foreach (var file in oldFiles)
                {
                    try
                    {
                        File.Delete(file);
                        filesDeleted++;
                    }
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                    {
                        LogManager.LogWarning($"Could not delete old temp file: {file}. Error: {ex.Message}");
                    }
                }
                if (filesDeleted > 0)
                {
                    LogManager.LogInfo($"Cleaned up {filesDeleted} old temporary files.");
                }
                return filesDeleted;
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.LogCritical(ex, "Failed to perform temp file cleanup.");
                return 0;
            }
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _maintenanceLock.Dispose();
                }

                _disposedValue = true;
            }
        }

        public void Dispose()
        {
            Dispose(disposing: true);
            GC.SuppressFinalize(this);
        }

    }
}