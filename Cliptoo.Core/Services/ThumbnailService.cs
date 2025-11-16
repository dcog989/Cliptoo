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

        public ThumbnailService(string appCachePath, IImageDecoder imageDecoder)
        {
            _cacheDir = Path.Combine(appCachePath, "Cliptoo", "Thumbnails");
            _previewCacheDir = Path.Combine(appCachePath, "Cliptoo", "Previews");
            Directory.CreateDirectory(_cacheDir);
            Directory.CreateDirectory(_previewCacheDir);

            _pngEncoder = new PngEncoder
            {
                CompressionLevel = PngCompressionLevel.Level6
            };
            _jpegEncoder = new JpegEncoder { Quality = 65 };
            _imageDecoder = imageDecoder;
        }

        private static string GetTargetExtension(string imagePath)
        {
            var sourceExtension = Path.GetExtension(imagePath).ToUpperInvariant();
            switch (sourceExtension)
            {
                // Photographic or complex images that benefit from JPEG
                case ".JPG":
                case ".JPEG":
                case ".BMP":
                case ".TIFF":
                case ".TIF":
                case ".HEIC":
                case ".HEIF":
                case ".JXL":
                case ".WEBP": // Can be lossy or lossless, but JPEG is a safe bet for previews.
                case ".RAW":
                case ".DNG":
                case ".CR2":
                case ".NEF":
                    return ".jpeg";

                // Images with transparency or simple graphics that benefit from PNG
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

            var cachePath = GenerateCachePath(imagePath, theme, size, cacheDirectory, targetExtension);

            if (File.Exists(cachePath))
            {
                LogManager.LogDebug($"THUMB_CACHE_DIAG: Hit for '{imagePath}' (Size: {size}, Theme: {theme ?? "none"}).");
                return cachePath;
            }
            LogManager.LogDebug($"THUMB_CACHE_DIAG: Miss for '{imagePath}' (Size: {size}, Theme: {theme ?? "none"}). Generating new image.");

            if (!File.Exists(imagePath)) return null;

            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            try
            {
                byte[]? outputBytes;
                if (sourceExtension == ".SVG")
                {
                    outputBytes = await ServiceUtils.GenerateSvgPreviewAsync(imagePath, size, theme).ConfigureAwait(false);
                }
                else
                {
                    using var stream = File.OpenRead(imagePath);
                    using var image = await _imageDecoder.DecodeAsync(stream, sourceExtension).ConfigureAwait(false);
                    if (image == null) return null;

                    await Task.Run(() => image.Mutate(x => x.Resize(new ResizeOptions
                    {
                        Size = new SixLabors.ImageSharp.Size(size, size),
                        Mode = ResizeMode.Max,
                        Sampler = KnownResamplers.Lanczos3
                    }))).ConfigureAwait(false);

                    using var ms = new MemoryStream();
                    if (targetExtension == ".jpeg")
                    {
                        await image.SaveAsync(ms, _jpegEncoder).ConfigureAwait(false);
                    }
                    else
                    {
                        await image.SaveAsync(ms, _pngEncoder).ConfigureAwait(false);
                    }
                    outputBytes = ms.ToArray();
                }

                if (outputBytes != null)
                {
                    await File.WriteAllBytesAsync(cachePath, outputBytes).ConfigureAwait(false);
                    return cachePath;
                }
            }
            catch (UnknownImageFormatException)
            {
                LogManager.LogDebug($"Unsupported image format for thumbnail generation: {imagePath}");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or NotSupportedException or ImageFormatException)
            {
                LogManager.LogCritical(ex, $"Image processing failed for {imagePath}");
            }
            finally
            {
                stopwatch.Stop();
                LogManager.LogDebug($"PERF_DIAG: Thumbnail generation for '{imagePath}' took {stopwatch.ElapsedMilliseconds}ms.");
            }

            return null;
        }

        public Task<string?> GetThumbnailAsync(string imagePath, string? theme) => GetImageInternalAsync(imagePath, theme, ThumbnailSize, _cacheDir);

        public Task<string?> GetImagePreviewAsync(string imagePath, uint largestDimension, string? theme) => GetImageInternalAsync(imagePath, theme, (int)largestDimension, _previewCacheDir);

        public void ClearCache()
        {
            try
            {
                ServiceUtils.DeleteDirectoryContents(_cacheDir);
                ServiceUtils.DeleteDirectoryContents(_previewCacheDir);
                LogManager.LogInfo("Thumbnail and preview caches cleared successfully.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.LogCritical(ex, "Failed to clear caches.");
            }
        }

        public async Task<int> PruneCacheAsync(IAsyncEnumerable<string> validImagePaths, uint previewSize)
        {
            ArgumentNullException.ThrowIfNull(validImagePaths);

            var validCacheFiles = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var enumerator = validImagePaths.GetAsyncEnumerator();
            try
            {
                while (await enumerator.MoveNextAsync().ConfigureAwait(false))
                {
                    var imagePath = enumerator.Current;
                    var sourceExtension = Path.GetExtension(imagePath).ToUpperInvariant();
                    var targetExtension = GetTargetExtension(imagePath);

                    validCacheFiles.Add(GenerateCachePath(imagePath, null, ThumbnailSize, _cacheDir, targetExtension));
                    validCacheFiles.Add(GenerateCachePath(imagePath, null, (int)previewSize, _previewCacheDir, targetExtension));

                    if (sourceExtension == ".SVG")
                    {
                        validCacheFiles.Add(GenerateCachePath(imagePath, "light", ThumbnailSize, _cacheDir, targetExtension));
                        validCacheFiles.Add(GenerateCachePath(imagePath, "dark", ThumbnailSize, _cacheDir, targetExtension));
                        validCacheFiles.Add(GenerateCachePath(imagePath, "light", (int)previewSize, _previewCacheDir, targetExtension));
                        validCacheFiles.Add(GenerateCachePath(imagePath, "dark", (int)previewSize, _previewCacheDir, targetExtension));
                    }
                }
            }
            finally
            {
                await enumerator.DisposeAsync().ConfigureAwait(false);
            }


            int filesDeleted = 0;
            filesDeleted += await ServiceUtils.PruneDirectoryAsync(_cacheDir, validCacheFiles).ConfigureAwait(false);
            filesDeleted += await ServiceUtils.PruneDirectoryAsync(_previewCacheDir, validCacheFiles).ConfigureAwait(false);

            return filesDeleted;
        }

        private static string GenerateCachePath(string imagePath, string? theme, int size, string cacheDirectory, string targetExtension)
        {
            var sourceExtension = Path.GetExtension(imagePath).ToUpperInvariant();
            var cacheKey = (sourceExtension == ".SVG" && !string.IsNullOrEmpty(theme))
                ? $"{imagePath}_{theme}_{size}"
                : $"{imagePath}_{size}";
            return ServiceUtils.GetCachePath(cacheKey, cacheDirectory, targetExtension);
        }
    }
}