using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using System.Xml;
using Cliptoo.Core.Logging;
using SixLabors.ImageSharp;
using SixLabors.ImageSharp.PixelFormats;
using SkiaSharp;
using Svg.Skia;

namespace Cliptoo.Core.Services
{
    public static class ServiceUtils
    {
        public static Image<Bgra32>? RenderSvgToImageSharp(string svgContent, int size)
        {
            try
            {
                using var skSvg = new SKSvg();
                using var picture = skSvg.FromSvg(svgContent);
                if (picture is null || picture.CullRect.Width <= 0 || picture.CullRect.Height <= 0) return null;

                var info = new SKImageInfo(size, size, SKColorType.Bgra8888, SKAlphaType.Premul);
                using var surface = SKSurface.Create(info);
                var canvas = surface.Canvas;
                canvas.Clear(SKColors.Transparent);
                using (var paint = new SKPaint { IsAntialias = true })
                {
                    var matrix = SKMatrix.CreateScale((float)size / picture.CullRect.Width, (float)size / picture.CullRect.Height);
                    canvas.DrawPicture(picture, matrix, paint);
                }

                using var skImage = surface.Snapshot();
                var skPixmap = skImage.PeekPixels();
                if (skPixmap == null) return null;

                var pixelBytes = skPixmap.GetPixelSpan().ToArray();
                return Image.LoadPixelData<Bgra32>(pixelBytes, size, size);
            }
            catch (Exception ex) when (ex is XmlException or NullReferenceException)
            {
                LogManager.LogCritical(ex, "SkiaSharp SVG rendering failed.");
                return null;
            }
        }

        public static string GetCachePath(string key, string directory, string extension)
        {
            var hash = SHA256.HashData(Encoding.UTF8.GetBytes(key));
            var hex = Convert.ToHexString(hash).ToUpperInvariant();
            return Path.Combine(directory, $"{hex}{extension}");
        }

        public static void DeleteDirectoryContents(string path)
        {
            var directory = new DirectoryInfo(path);
            if (!directory.Exists) return;

            foreach (var file in directory.EnumerateFiles())
            {
                try
                {
                    file.Delete();
                }
                catch (IOException ex)
                {
                    LogManager.LogWarning($"Failed to delete cached file: {file.FullName}. Error: {ex.Message}");
                }
            }
            foreach (var dir in directory.EnumerateDirectories())
            {
                try
                {
                    dir.Delete(true);
                }
                catch (IOException ex)
                {
                    LogManager.LogWarning($"Failed to delete cached directory: {dir.FullName}. Error: {ex.Message}");
                }
            }
        }

        public static async Task<byte[]?> GenerateSvgPreviewAsync(string svgSource, int size, string? theme, bool isContentString = false)
        {
            ArgumentNullException.ThrowIfNull(svgSource);
            try
            {
                string svgContent = isContentString ? svgSource : await File.ReadAllTextAsync(svgSource).ConfigureAwait(false);
                if (!string.IsNullOrEmpty(theme))
                {
                    var foregroundColor = theme == "dark" ? "#FFFFFF" : "#000000";
                    svgContent = svgContent.Replace("currentColor", foregroundColor, StringComparison.OrdinalIgnoreCase);
                }

                using var imageSharpImage = await Task.Run(() => RenderSvgToImageSharp(svgContent, size)).ConfigureAwait(false);
                if (imageSharpImage == null) return null;

                using var ms = new MemoryStream();
                var pngEncoder = new SixLabors.ImageSharp.Formats.Png.PngEncoder
                {
                    CompressionLevel = SixLabors.ImageSharp.Formats.Png.PngCompressionLevel.Level6
                };
                await imageSharpImage.SaveAsync(ms, pngEncoder).ConfigureAwait(false);
                return ms.ToArray();
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or XmlException)
            {
                var previewContent = isContentString ? (svgSource.Length > 500 ? svgSource.Substring(0, 500) : svgSource) : $"File: {svgSource}";
                LogManager.LogCritical(ex, $"SVG preview generation failed for {previewContent}");
                return null;
            }
        }

        public static async Task<int> PruneDirectoryAsync(string directoryPath, HashSet<string> validFiles)
        {
            ArgumentNullException.ThrowIfNull(validFiles);

            int count = 0;
            if (!Directory.Exists(directoryPath)) return 0;
            var filesToDelete = new List<string>();
            try
            {
                foreach (var file in Directory.EnumerateFiles(directoryPath))
                {
                    if (!validFiles.Contains(file))
                    {
                        filesToDelete.Add(file);
                    }
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or DirectoryNotFoundException)
            {
                LogManager.LogCritical(ex, $"Failed to enumerate files for pruning in {directoryPath}");
                return 0;
            }

            foreach (var file in filesToDelete)
            {
                try
                {
                    await Task.Run(() => File.Delete(file)).ConfigureAwait(false);
                    count++;
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                {
                    LogManager.LogWarning($"Could not delete orphaned cache file: {file}. Error: {ex.Message}");
                }
            }
            return count;
        }

    }
}