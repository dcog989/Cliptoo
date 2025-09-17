using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Native;
using Cliptoo.Core.Native.Models;
using Cliptoo.Core.Services;
using Cliptoo.Core.Services.Models;
using Microsoft.Data.Sqlite;
using SixLabors.ImageSharp;

namespace Cliptoo.Core
{
    public class CliptooController : IDisposable
    {
        public event EventHandler<ProcessingFailedEventArgs>? ProcessingFailed;
        private readonly IClipDataService _clipDataService;
        private readonly IContentProcessor _contentProcessor;
        private readonly IThumbnailService _thumbnailService;
        private readonly ISettingsService _settingsService;
        private readonly IFileTypeClassifier _fileTypeClassifier;
        private readonly IDatabaseService _databaseService;
        private readonly IAppInteractionService _appInteractionService;
        private readonly System.Timers.Timer _cleanupTimer;
        private readonly string _clipboardImageCachePath;
        private bool _isInitialized;

        public IClipboardMonitor ClipboardMonitor { get; }

        public CliptooController(
            IClipDataService clipDataService,
            IContentProcessor contentProcessor,
            IThumbnailService thumbnailService,
            IClipboardMonitor clipboardMonitor,
            ISettingsService settingsService,
            IFileTypeClassifier fileTypeClassifier,
            IDatabaseService databaseService,
            IAppInteractionService appInteractionService)
        {
            _clipDataService = clipDataService;
            _contentProcessor = contentProcessor;
            _thumbnailService = thumbnailService;
            ClipboardMonitor = clipboardMonitor;
            _settingsService = settingsService;
            _fileTypeClassifier = fileTypeClassifier;
            _databaseService = databaseService;
            _appInteractionService = appInteractionService;

            var appDataLocalPath = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            _clipboardImageCachePath = Path.Combine(appDataLocalPath, "Cliptoo", "ClipboardImageCache");
            Directory.CreateDirectory(_clipboardImageCachePath);

            _cleanupTimer = new System.Timers.Timer();
            _cleanupTimer.Elapsed += OnCleanupTimerElapsed;
            _cleanupTimer.Interval = TimeSpan.FromHours(2).TotalMilliseconds;
        }

        public Task InitializeAsync()
        {
            LogManager.LogDebug("Controller initializing...");
            var settings = _settingsService.Settings;
            LogManager.Configure(settings.LoggingLevel, settings.LogRetentionDays);
            _databaseService.CleanupTempFiles();

            ClipboardMonitor.ClipboardChanged += OnClipboardChangedAsync;
            _clipDataService.ClipDeleted += OnClipDeleted;
            _fileTypeClassifier.FileTypesChanged += OnFileTypesChanged;
            _cleanupTimer.Start();

            LogManager.LogDebug("Controller initialized.");
            _isInitialized = true;
            return Task.CompletedTask;
        }

        private void OnFileTypesChanged(object? sender, EventArgs e)
        {
            if (!_isInitialized)
            {
                LogManager.LogDebug("OnFileTypesChanged skipped during initialization.");
                return;
            }

            Task.Run(async () =>
            {
                try
                {
                    LogManager.LogInfo("File type definitions changed, starting re-classification of existing clips.");
                    int count = await _databaseService.ReclassifyAllClipsAsync().ConfigureAwait(false);
                    LogManager.LogInfo($"Re-classification complete. {count} clips updated.");
                }
                catch (Exception ex) when (ex is IOException or SqliteException)
                {
                    LogManager.LogCritical(ex, "Error during background re-classification.");
                }
            });
        }

        private void OnCleanupTimerElapsed(object? sender, System.Timers.ElapsedEventArgs e)
        {
            Task.Run(async () =>
            {
                try
                {
                    if (_appInteractionService.IsUiInteractive || (DateTime.UtcNow - _appInteractionService.LastActivityTimestamp) < TimeSpan.FromMinutes(5))
                    {
                        LogManager.LogDebug("Cleanup skipped due to recent activity or visible UI.");
                        return;
                    }

                    var stats = await _databaseService.GetStatsAsync().ConfigureAwait(false);
                    var lastCleanup = stats.LastCleanupTimestamp ?? DateTime.MinValue;

                    if ((DateTime.UtcNow - lastCleanup) > TimeSpan.FromDays(1))
                    {
                        LogManager.LogInfo("Idle timer check: Last heavy maintenance was over 24 hours ago. Triggering routine.");
                        await _databaseService.RunHeavyMaintenanceNowAsync().ConfigureAwait(false);
                    }
                    else
                    {
                        LogManager.LogDebug("Idle timer check: Heavy maintenance not due yet.");
                    }
                }
                catch (Exception ex) when (ex is IOException or SqliteException)
                {
                    LogManager.LogCritical(ex, "Error during scheduled cleanup.");
                }
            });
        }

        private void OnClipboardChangedAsync(object? sender, ClipboardChangedEventArgs e)
        {
            Task.Run(async () =>
            {
                try
                {
                    await ProcessClipboardChange(e).ConfigureAwait(false);
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or NotSupportedException or UnknownImageFormatException or ImageFormatException or SqliteException)
                {
                    LogManager.LogCritical(ex, "Error in ProcessClipboardChange. The operation will be skipped, but the application will continue.");
                    ProcessingFailed?.Invoke(this, new ProcessingFailedEventArgs("Failed to Save Clip", "Could not process and save the latest clipboard item. See logs for details."));
                }
            });
        }

        private void OnClipDeleted(object? sender, EventArgs e)
        {
            ClipboardMonitor.ForgetLastContentHashes();
        }

        private async Task ProcessClipboardChange(ClipboardChangedEventArgs e)
        {
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            ProcessingResult? result = null;
            var settings = _settingsService.Settings;
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
                    LogManager.LogWarning($"Image clip of size {imageBytes.Length} bytes exceeds limit of {maxBytes} bytes. Clip will be ignored.");
                    return;
                }

                var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(imageBytes)).ToUpperInvariant();
                var imagePath = Path.Combine(_clipboardImageCachePath, $"{hash}.png");

                if (!File.Exists(imagePath))
                {
                    await File.WriteAllBytesAsync(imagePath, imageBytes).ConfigureAwait(false);
                }

                await _thumbnailService.GetThumbnailAsync(imagePath, null).ConfigureAwait(false);
                result = new ProcessingResult(AppConstants.ClipTypes.Image, imagePath);
            }
            else if (e.ContentType == ClipboardContentType.FileDrop)
            {
                var content = (string)e.Content;
                var paths = content.Split(new[] { Environment.NewLine }, StringSplitOptions.RemoveEmptyEntries);

                if (paths.Length == 1)
                {
                    var path = paths[0].Trim();
                    if (path.EndsWith(".url", StringComparison.OrdinalIgnoreCase))
                    {
                        var extractedUrl = ParseUrlFile(path);
                        if (!string.IsNullOrEmpty(extractedUrl))
                        {
                            result = new ProcessingResult(AppConstants.ClipTypes.Link, extractedUrl, false, Path.GetFileName(path));
                        }
                    }
                    if (result == null) // If not a .url file or parsing failed
                    {
                        var fileType = _fileTypeClassifier.Classify(path);
                        result = new ProcessingResult(fileType, content);
                    }
                }
                else if (paths.Length > 1)
                {
                    result = _contentProcessor.Process(content);
                }
            }


            if (result != null)
            {
                bool finalWasTrimmed = result.SourceHadWhitespaceTrimmed || wasTruncated;
                string? sourceApp = result.SourceAppOverride ?? e.SourceApp;
                await _clipDataService.AddClipAsync(
                    result.Content,
                    result.ClipType,
                    sourceApp,
                    finalWasTrimmed).ConfigureAwait(false);
                _appInteractionService.NotifyUiActivity();
            }
            stopwatch.Stop();
            LogManager.LogDebug($"PERF_DIAG: OnClipboardChangedAsync processed in {stopwatch.ElapsedMilliseconds}ms.");
        }

        private static (string, bool) TruncateText(string text, long maxBytes, string logContext)
        {
            if (Encoding.UTF8.GetByteCount(text) <= maxBytes)
            {
                return (text, false);
            }

            var encoder = Encoding.UTF8.GetEncoder();
            var bytes = new byte[maxBytes];
            encoder.Convert(text.AsSpan(), bytes, true, out int charsUsed, out _, out _);
            var truncatedText = text.Substring(0, charsUsed);
            LogManager.LogWarning($"{logContext} truncated to {maxBytes} bytes.");
            return (truncatedText, true);
        }

        private static string? ParseUrlFile(string filePath)
        {
            try
            {
                var lines = File.ReadAllLines(filePath);
                foreach (var line in lines)
                {
                    if (line.Trim().StartsWith("URL=", StringComparison.OrdinalIgnoreCase))
                    {
                        return line.Substring(line.IndexOf('=', StringComparison.Ordinal) + 1).Trim();
                    }
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException)
            {
                LogManager.LogCritical(ex, $"Failed to parse .url file: {filePath}");
            }
            return null;
        }

        public void Dispose()
        {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        protected virtual void Dispose(bool disposing)
        {
            if (disposing)
            {
                LogManager.LogInfo("Cliptoo shutting down.");
                _cleanupTimer.Stop();
                _cleanupTimer.Dispose();
                _clipDataService.ClipDeleted -= OnClipDeleted;
                _fileTypeClassifier.FileTypesChanged -= OnFileTypesChanged;
                ClipboardMonitor.ClipboardChanged -= OnClipboardChangedAsync;
                ClipboardMonitor.Dispose();
            }
        }

    }
}