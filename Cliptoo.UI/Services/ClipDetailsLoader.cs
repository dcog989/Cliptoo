using System.Collections.Concurrent;
using System.Globalization;
using System.IO;
using System.Net.Http;
using System.Text;
using Cliptoo.Core;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.ViewModels;
using SixLabors.ImageSharp;
using Svg.Skia;

namespace Cliptoo.UI.Services
{
    internal class ClipDetailsLoader : IClipDetailsLoader
    {
        private readonly IImageDecoder _imageDecoder;
        private readonly ConcurrentDictionary<string, (DateTime Timestamp, (long Size, int FileCount, int FolderCount, bool WasLimited) Result)> _directorySizeCache = new();
        private static readonly TimeSpan CacheDuration = TimeSpan.FromSeconds(30);
        private static readonly TimeSpan IoTimeout = TimeSpan.FromMilliseconds(1500);

        public ClipDetailsLoader(IImageDecoder imageDecoder)
        {
            _imageDecoder = imageDecoder;
        }

        public async Task<string?> GetThumbnailAsync(ClipViewModel vm, IThumbnailService thumbnailService, IWebMetadataService webMetadataService, string theme)
        {
            ArgumentNullException.ThrowIfNull(vm);
            ArgumentNullException.ThrowIfNull(thumbnailService);
            ArgumentNullException.ThrowIfNull(webMetadataService);

            var extension = Path.GetExtension(vm.Content)?.ToUpperInvariant() ?? string.Empty;

            if (vm.ClipType == AppConstants.ClipTypeImage)
            {
                if (vm.Content.StartsWith(@"\\", StringComparison.Ordinal))
                {
                    return null;
                }

                return await thumbnailService.GetThumbnailAsync(vm.Content, extension == ".SVG" ? theme : null).ConfigureAwait(false);
            }
            if (vm.ClipType == AppConstants.ClipTypeLink && Uri.TryCreate(vm.Content, UriKind.Absolute, out var uri))
            {
                return await webMetadataService.GetFaviconAsync(uri, theme).ConfigureAwait(false);
            }
            return null;
        }

        public async Task<string?> GetImagePreviewAsync(ClipViewModel vm, IThumbnailService thumbnailService, uint size, string theme)
        {
            ArgumentNullException.ThrowIfNull(vm);
            ArgumentNullException.ThrowIfNull(thumbnailService);

            if (!vm.IsImage) return null;
            if (vm.Content.StartsWith(@"\\", StringComparison.Ordinal)) return null;

            var extension = Path.GetExtension(vm.Content)?.ToUpperInvariant() ?? string.Empty;
            return await thumbnailService.GetImagePreviewAsync(vm.Content, size, extension == ".SVG" ? theme : null).ConfigureAwait(false);
        }

        public async Task<string?> GetPageTitleAsync(ClipViewModel vm, IWebMetadataService webMetadataService, CancellationToken token)
        {
            ArgumentNullException.ThrowIfNull(vm);
            ArgumentNullException.ThrowIfNull(webMetadataService);

            if (!vm.IsLinkToolTip || string.IsNullOrEmpty(vm.Content) || !Uri.TryCreate(vm.Content, UriKind.Absolute, out var uri)) return null;

            try
            {
                var title = await webMetadataService.GetPageTitleAsync(uri).ConfigureAwait(false);
                if (!token.IsCancellationRequested)
                {
                    return string.IsNullOrWhiteSpace(title) ? vm.DisplayContent : title;
                }
            }
            catch (HttpRequestException ex)
            {
                if (!token.IsCancellationRequested)
                {
                    LogManager.LogWarning($"Failed to load page title. Error: {ex.Message}");
                    return vm.DisplayContent;
                }
            }

            return null;
        }

        public async Task<(string? properties, string? typeInfo, bool isMissing)> GetFilePropertiesAsync(ClipViewModel vm, CancellationToken token)
        {
            ArgumentNullException.ThrowIfNull(vm);

            if (!vm.IsFileBased || string.IsNullOrEmpty(vm.Content))
            {
                return (null, null, false);
            }

            string? fileProperties = null;
            string? fileTypeInfo = null;
            bool isMissing = false;

            try
            {
                var path = vm.Content.Trim();
                bool isNetworkPath = path.StartsWith(@"\\", StringComparison.Ordinal);
                var sb = new StringBuilder();

                using var timeoutCts = CancellationTokenSource.CreateLinkedTokenSource(token);
                timeoutCts.CancelAfter(IoTimeout);

                await Task.Run(async () =>
                {
                    if (timeoutCts.Token.IsCancellationRequested) return;

                    bool existsAsDir = Directory.Exists(path);
                    bool existsAsFile = !existsAsDir && File.Exists(path);

                    if (existsAsDir)
                    {
                        var dirInfo = new DirectoryInfo(path);
                        sb.AppendLine(CultureInfo.InvariantCulture, $"Modified: {dirInfo.LastWriteTime:yyyy-MM-dd HH:mm}");
                        fileTypeInfo = isNetworkPath ? $"Remote Folder" : FormatUtils.GetFriendlyClipTypeName(vm.ClipType);

                        if (isNetworkPath)
                        {
                            sb.AppendLine("Size: (network path - calculation skipped)");
                        }
                        else
                        {
                            try
                            {
                                (long size, int fileCount, int folderCount, bool wasLimited) dirStats;
                                if (_directorySizeCache.TryGetValue(path, out var cachedEntry) && (DateTime.UtcNow - cachedEntry.Timestamp) < CacheDuration)
                                {
                                    dirStats = cachedEntry.Result;
                                }
                                else
                                {
                                    dirStats = await Task.Run(() => CalculateDirectorySize(dirInfo, timeoutCts.Token), timeoutCts.Token).ConfigureAwait(false);
                                    if (timeoutCts.Token.IsCancellationRequested) return;
                                    _directorySizeCache[path] = (DateTime.UtcNow, dirStats);
                                }

                                var sizeString = dirStats.wasLimited ? $"> {FormatUtils.FormatBytes(dirStats.size)}" : FormatUtils.FormatBytes(dirStats.size);
                                sb.AppendLine(CultureInfo.InvariantCulture, $"Size: {sizeString}");
                                sb.AppendLine(CultureInfo.InvariantCulture, $"Contains: {dirStats.fileCount:N0} files, {dirStats.folderCount:N0} folders");
                            }
                            catch (OperationCanceledException)
                            {
                                sb.AppendLine("Size: (calculation timed out)");
                            }
                            catch (UnauthorizedAccessException)
                            {
                                sb.AppendLine("Size: (access denied)");
                            }
                        }
                    }
                    else if (existsAsFile)
                    {
                        var fileInfo = new FileInfo(path);
                        sb.AppendLine(CultureInfo.InvariantCulture, $"Size: {FormatUtils.FormatBytes(fileInfo.Length)}");
                        sb.AppendLine(CultureInfo.InvariantCulture, $"Modified: {fileInfo.LastWriteTime:yyyy-MM-dd HH:mm}");
                        fileTypeInfo = isNetworkPath
                            ? $"Remote File ({fileInfo.Extension.ToUpperInvariant()})"
                            : $"{fileInfo.Extension.ToUpperInvariant()} ({FormatUtils.GetFriendlyClipTypeName(vm.ClipType)})";

                        if (vm.IsImage && !isNetworkPath)
                        {
                            try
                            {
                                var extension = Path.GetExtension(path).ToUpperInvariant();
                                using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 4096, useAsync: true);

                                if (extension == ".SVG")
                                {
                                    using var skSvg = new SKSvg();
                                    if (skSvg.Load(stream) is { } picture)
                                    {
                                        sb.AppendLine(CultureInfo.InvariantCulture, $"Dimensions: {(int)picture.CullRect.Width} x {(int)picture.CullRect.Height}");
                                    }
                                }
                                else if (extension == ".JXL")
                                {
                                    using var image = await _imageDecoder.DecodeAsync(stream, extension).ConfigureAwait(false);
                                    if (image != null)
                                    {
                                        sb.AppendLine(CultureInfo.InvariantCulture, $"Dimensions: {image.Width} x {image.Height}");
                                    }
                                }
                                else
                                {
                                    var imageInfo = await Image.IdentifyAsync(stream, timeoutCts.Token).ConfigureAwait(false);
                                    if (imageInfo != null)
                                    {
                                        sb.AppendLine(CultureInfo.InvariantCulture, $"Dimensions: {imageInfo.Width} x {imageInfo.Height}");
                                    }
                                }
                            }
                            catch (Exception) { }
                        }
                    }
                    else
                    {
                        isMissing = true;
                    }

                    if (!timeoutCts.Token.IsCancellationRequested && !isMissing)
                    {
                        fileProperties = sb.ToString().Trim();
                    }
                }, timeoutCts.Token).ConfigureAwait(false);
            }
            catch (OperationCanceledException)
            {
                if (!token.IsCancellationRequested)
                {
                    fileProperties = "Metadata loading timed out (possible slow network).";
                    fileTypeInfo = "Unknown System Path";
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                if (!token.IsCancellationRequested)
                {
                    fileProperties = "Error reading properties.";
                }
                LogManager.LogCritical(ex, "Failed to load file properties.");
            }

            return (fileProperties, fileTypeInfo, isMissing);
        }

        private static (long Size, int FileCount, int FolderCount, bool WasLimited) CalculateDirectorySize(DirectoryInfo dirInfo, CancellationToken token, int depth = 0)
        {
            const int maxDepth = 4;
            if (depth > maxDepth || token.IsCancellationRequested)
            {
                return (0, 0, 0, true);
            }

            long size = 0;
            int fileCount = 0;
            int folderCount = 0;
            bool wasLimited = false;

            try
            {
                foreach (var file in dirInfo.EnumerateFiles())
                {
                    token.ThrowIfCancellationRequested();
                    size += file.Length;
                    fileCount++;
                }

                foreach (var dir in dirInfo.EnumerateDirectories())
                {
                    token.ThrowIfCancellationRequested();
                    var subDirSize = CalculateDirectorySize(dir, token, depth + 1);
                    size += subDirSize.Size;
                    fileCount += subDirSize.FileCount;
                    folderCount += subDirSize.FolderCount + 1;
                    if (subDirSize.WasLimited)
                    {
                        wasLimited = true;
                    }
                }
            }
            catch (UnauthorizedAccessException) { }
            catch (DirectoryNotFoundException) { }

            return (size, fileCount, folderCount, wasLimited);
        }
    }
}
