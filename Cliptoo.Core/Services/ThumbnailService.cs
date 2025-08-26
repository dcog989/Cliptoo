using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Cliptoo.Core.Configuration;
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

        public ThumbnailService(string appCachePath)
        {
            _cacheDir = Path.Combine(appCachePath, "Cliptoo", "Thumbnails");
            _previewCacheDir = Path.Combine(appCachePath, "Cliptoo", "Previews");
            Directory.CreateDirectory(_cacheDir);
            Directory.CreateDirectory(_previewCacheDir);

            _pngEncoder = new PngEncoder { CompressionLevel = PngCompressionLevel.Level6 };
            _jpegEncoder = new JpegEncoder { Quality = 50 };
        }

        private string GetTargetExtension(string imagePath)
        {
            var sourceExtension = Path.GetExtension(imagePath).ToLowerInvariant();
            switch (sourceExtension)
            {
                // Photographic or complex images that benefit from JPEG
                case ".jpg":
                case ".jpeg":
                case ".bmp":
                case ".tiff":
                case ".tif":
                case ".heic":
                case ".heif":
                case ".jxl":
                case ".webp": // Can be lossy or lossless, but JPEG is a safe bet for previews.
                case ".raw":
                case ".dng":
                case ".cr2":
                case ".nef":
                    return ".jpeg";

                // Images with transparency or simple graphics that benefit from PNG
                case ".png":
                case ".gif":
                case ".svg":
                case ".ico":
                default:
                    return ".png";
            }
        }

        private async Task<string?> GetImageInternalAsync(string imagePath, string? theme, int size, string cacheDirectory)
        {
            var sourceExtension = Path.GetExtension(imagePath).ToLowerInvariant();
            var targetExtension = GetTargetExtension(imagePath);

            var cacheKey = (sourceExtension == ".svg" && !string.IsNullOrEmpty(theme)) ? $"{imagePath}_{theme}_{size}" : $"{imagePath}_{size}";
            var cachePath = ServiceUtils.GetCachePath(cacheKey, cacheDirectory, targetExtension);

            if (File.Exists(cachePath)) return cachePath;
            if (!File.Exists(imagePath)) return null;

            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            try
            {
                byte[]? outputBytes;
                if (sourceExtension == ".svg")
                {
                    outputBytes = await ServiceUtils.GenerateSvgPreviewAsync(imagePath, size, theme).ConfigureAwait(false);
                }
                else
                {
                    using var image = await ImageDecoder.DecodeAsync(imagePath).ConfigureAwait(false);
                    if (image == null) return null;

                    image.Mutate(x => x.Resize(new ResizeOptions
                    {
                        Size = new SixLabors.ImageSharp.Size(size, size),
                        Mode = ResizeMode.Max
                    }));

                    await using var ms = new MemoryStream();
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
            catch (SixLabors.ImageSharp.UnknownImageFormatException)
            {
                LogManager.LogDebug($"Unsupported image format for thumbnail generation: {imagePath}");
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, $"Image processing failed for {imagePath}");
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
                LogManager.Log("Thumbnail and preview caches cleared successfully.");
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, "Failed to clear caches.");
            }
        }

        public async Task<int> PruneCacheAsync(IAsyncEnumerable<string> validImagePaths, uint previewSize)
        {
            var validCacheFiles = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

            await foreach (var imagePath in validImagePaths)
            {
                var sourceExtension = Path.GetExtension(imagePath).ToLowerInvariant();
                var targetExtension = GetTargetExtension(imagePath);

                validCacheFiles.Add(GenerateCachePath(imagePath, null, ThumbnailSize, _cacheDir, targetExtension));
                validCacheFiles.Add(GenerateCachePath(imagePath, null, (int)previewSize, _previewCacheDir, targetExtension));

                if (sourceExtension == ".svg")
                {
                    validCacheFiles.Add(GenerateCachePath(imagePath, "light", ThumbnailSize, _cacheDir, targetExtension));
                    validCacheFiles.Add(GenerateCachePath(imagePath, "dark", ThumbnailSize, _cacheDir, targetExtension));
                    validCacheFiles.Add(GenerateCachePath(imagePath, "light", (int)previewSize, _previewCacheDir, targetExtension));
                    validCacheFiles.Add(GenerateCachePath(imagePath, "dark", (int)previewSize, _previewCacheDir, targetExtension));
                }
            }

            int filesDeleted = 0;
            filesDeleted += await ServiceUtils.PruneDirectoryAsync(_cacheDir, validCacheFiles).ConfigureAwait(false);
            filesDeleted += await ServiceUtils.PruneDirectoryAsync(_previewCacheDir, validCacheFiles).ConfigureAwait(false);

            return filesDeleted;
        }

        private string GenerateCachePath(string imagePath, string? theme, int size, string cacheDirectory, string targetExtension)
        {
            var sourceExtension = Path.GetExtension(imagePath).ToLowerInvariant();
            var cacheKey = (sourceExtension == ".svg" && !string.IsNullOrEmpty(theme))
                ? $"{imagePath}_{theme}_{size}"
                : $"{imagePath}_{size}";
            return ServiceUtils.GetCachePath(cacheKey, cacheDirectory, targetExtension);
        }
    }
}