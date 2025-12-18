using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Cliptoo.Core.Logging;
using SixLabors.ImageSharp;
using SixLabors.ImageSharp.Formats.Jpeg;
using SixLabors.ImageSharp.Formats.Png;
using SixLabors.ImageSharp.Processing;

namespace Cliptoo.Core.Services
{
    public class ThumbnailService : IThumbnailService
    {
        private const int ThumbnailSize = 32;
        private readonly string _cacheDir;
        private readonly string _previewCacheDir;
        private readonly PngEncoder _pngEncoder;
        private readonly JpegEncoder _jpegEncoder;
        private readonly IImageDecoder _imageDecoder;
        private readonly LruCache<string, string> _memoryPathCache;

        public ThumbnailService(string appCachePath, IImageDecoder imageDecoder)
        {
            _cacheDir = Path.Combine(appCachePath, "Cliptoo", "Thumbnails");
            _previewCacheDir = Path.Combine(appCachePath, "Cliptoo", "Previews");
            Directory.CreateDirectory(_cacheDir);
            Directory.CreateDirectory(_previewCacheDir);

            _pngEncoder = new PngEncoder { CompressionLevel = PngCompressionLevel.Level6 };
            _jpegEncoder = new JpegEncoder { Quality = 65 };
            _imageDecoder = imageDecoder;
            _memoryPathCache = new LruCache<string, string>(1000);
        }

        private static string GetTargetExtension(string imagePath)
        {
            var sourceExtension = Path.GetExtension(imagePath).ToUpperInvariant();
            switch (sourceExtension)
            {
                case ".JPG":
                case ".JPEG":
                case ".BMP":
                case ".TIFF":
                case ".TIF":
                case ".HEIC":
                case ".HEIF":
                case ".JXL":
                case ".WEBP":
                case ".RAW":
                case ".DNG":
                case ".CR2":
                case ".NEF":
                    return ".jpeg";
                case ".PNG":
                case ".GIF":
                case ".SVG":
                case ".ICO":
                default:
                    return ".png";
            }
        }

        private async Task<string?> GetImageInternalAsync(string imagePath, string? theme, int size, string cacheDirectory)
        {
            var sourceExtension = Path.GetExtension(imagePath).ToUpperInvariant();
            var targetExtension = GetTargetExtension(imagePath);
            var cacheKey = (sourceExtension == ".SVG" && !string.IsNullOrEmpty(theme))
                ? $"{imagePath}_{theme}_{size}"
                : $"{imagePath}_{size}";

            if (_memoryPathCache.TryGetValue(cacheKey, out var memoryCachedPath) && File.Exists(memoryCachedPath))
            {
                return memoryCachedPath;
            }

            var cachePath = ServiceUtils.GetCachePath(cacheKey, cacheDirectory, targetExtension);

            if (File.Exists(cachePath))
            {
                _memoryPathCache.Add(cacheKey, cachePath);
                return cachePath;
            }

            if (!File.Exists(imagePath)) return null;

            try
            {
                byte[]? outputBytes;
                if (sourceExtension == ".SVG")
                {
                    outputBytes = await ServiceUtils.GenerateSvgPreviewAsync(imagePath, size, theme).ConfigureAwait(false);
                }
                else
                {
                    outputBytes = await Task.Run(async () =>
                    {
                        try
                        {
                            using var stream = new FileStream(imagePath, FileMode.Open, FileAccess.Read, FileShare.Read, 4096, useAsync: true);
                            using var image = await _imageDecoder.DecodeAsync(stream, sourceExtension).ConfigureAwait(false);
                            if (image == null) return null;

                            image.Mutate(x => x.Resize(new ResizeOptions
                            {
                                Size = new SixLabors.ImageSharp.Size(size, size),
                                Mode = ResizeMode.Max,
                                Sampler = KnownResamplers.Lanczos3
                            }));

                            using var ms = new MemoryStream();
                            if (targetExtension == ".jpeg") await image.SaveAsync(ms, _jpegEncoder).ConfigureAwait(false);
                            else await image.SaveAsync(ms, _pngEncoder).ConfigureAwait(false);
                            return ms.ToArray();
                        }
                        catch (Exception) { return null; }
                    }).ConfigureAwait(false);
                }

                if (outputBytes != null)
                {
                    await File.WriteAllBytesAsync(cachePath, outputBytes).ConfigureAwait(false);
                    _memoryPathCache.Add(cacheKey, cachePath);
                    return cachePath;
                }
            }
            catch (Exception ex)
            {
                LogManager.LogCritical(ex, $"Image processing failed for {imagePath}");
            }

            return null;
        }

        public Task<string?> GetThumbnailAsync(string imagePath, string? theme)
            => GetImageInternalAsync(imagePath, theme, ThumbnailSize, _cacheDir);

        public Task<string?> GetImagePreviewAsync(string imagePath, uint largestDimension, string? theme)
            => GetImageInternalAsync(imagePath, theme, (int)largestDimension, _previewCacheDir);

        public void ClearCache()
        {
            try
            {
                ServiceUtils.DeleteDirectoryContents(_cacheDir);
                ServiceUtils.DeleteDirectoryContents(_previewCacheDir);
                _memoryPathCache.Clear();
                LogManager.LogInfo("Thumbnail and preview caches cleared successfully.");
            }
            catch (Exception ex)
            {
                LogManager.LogCritical(ex, "Failed to clear caches.");
            }
        }

        public async Task<int> PruneCacheAsync(IAsyncEnumerable<string> validImagePaths, uint previewSize)
        {
            ArgumentNullException.ThrowIfNull(validImagePaths);
            var validCacheFiles = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            await foreach (var imagePath in validImagePaths.ConfigureAwait(false))
            {
                var targetExt = GetTargetExtension(imagePath);
                validCacheFiles.Add(ServiceUtils.GetCachePath($"{imagePath}_{ThumbnailSize}", _cacheDir, targetExt));
                validCacheFiles.Add(ServiceUtils.GetCachePath($"{imagePath}_{(int)previewSize}", _previewCacheDir, targetExt));

                if (Path.GetExtension(imagePath).Equals(".SVG", StringComparison.OrdinalIgnoreCase))
                {
                    validCacheFiles.Add(ServiceUtils.GetCachePath($"{imagePath}_light_{ThumbnailSize}", _cacheDir, targetExt));
                    validCacheFiles.Add(ServiceUtils.GetCachePath($"{imagePath}_dark_{ThumbnailSize}", _cacheDir, targetExt));
                    validCacheFiles.Add(ServiceUtils.GetCachePath($"{imagePath}_light_{(int)previewSize}", _previewCacheDir, targetExt));
                    validCacheFiles.Add(ServiceUtils.GetCachePath($"{imagePath}_dark_{(int)previewSize}", _previewCacheDir, targetExt));
                }
            }

            int deleted = await ServiceUtils.PruneDirectoryAsync(_cacheDir, validCacheFiles).ConfigureAwait(false);
            deleted += await ServiceUtils.PruneDirectoryAsync(_previewCacheDir, validCacheFiles).ConfigureAwait(false);

            _memoryPathCache.Clear();
            return deleted;
        }
    }
}
