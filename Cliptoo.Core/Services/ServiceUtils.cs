using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using Cliptoo.Core.Configuration;
using SkiaSharp;
using Svg.Skia;

namespace Cliptoo.Core.Services
{
    public static class ServiceUtils
    {
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
                catch (IOException) { }
            }
            foreach (var dir in directory.EnumerateDirectories())
            {
                try
                {
                    dir.Delete(true);
                }
                catch (IOException) { }
            }
        }

        public static async Task<byte[]?> GenerateSvgPreviewAsync(string svgSource, int size, string? theme, bool isContentString = false)
        {
            return await Task.Run(async () =>
            {
                try
                {
                    string svgContent = isContentString ? svgSource : await File.ReadAllTextAsync(svgSource).ConfigureAwait(false);
                    if (!string.IsNullOrEmpty(theme))
                    {
                        var foregroundColor = theme == "dark" ? "#FFFFFF" : "#000000";
                        svgContent = svgContent.Replace("currentColor", foregroundColor, StringComparison.OrdinalIgnoreCase);
                    }

                    using var skSvg = new SKSvg();
                    using var picture = skSvg.FromSvg(svgContent);
                    if (picture is null || picture.CullRect.Width <= 0 || picture.CullRect.Height <= 0) return null;

                    var scale = Math.Min(size / picture.CullRect.Width, size / picture.CullRect.Height);
                    var matrix = SKMatrix.CreateScale(scale, scale);
                    matrix.TransX = -picture.CullRect.Left * scale;
                    matrix.TransY = -picture.CullRect.Top * scale;

                    var finalWidth = (int)Math.Max(1, picture.CullRect.Width * scale);
                    var finalHeight = (int)Math.Max(1, picture.CullRect.Height * scale);

                    using var bitmap = new SKBitmap(finalWidth, finalHeight);
                    using var canvas = new SKCanvas(bitmap);
                    canvas.Clear(SKColors.Transparent);
                    using (var paint = new SKPaint { IsAntialias = true })
                    {
                        canvas.DrawPicture(picture, matrix, paint);
                    }

                    using var image = SKImage.FromBitmap(bitmap);
                    using var data = image.Encode(SKEncodedImageFormat.Png, 60);
                    return data.ToArray();
                }
                catch (Exception ex)
                {
                    var previewContent = isContentString ? (svgSource.Length > 500 ? svgSource.Substring(0, 500) : svgSource) : $"File: {svgSource}";
                    LogManager.Log(ex, $"SVG preview generation failed for {previewContent}");
                    return null;
                }
            });
        }

        public static async Task<int> PruneDirectoryAsync(string directoryPath, HashSet<string> validFiles)
        {
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

            catch (Exception ex)
            {
                LogManager.Log(ex, $"Failed to enumerate files for pruning in {directoryPath}");
                return 0;
            }

            foreach (var file in filesToDelete)
            {
                try
                {
                    await Task.Run(() => File.Delete(file)).ConfigureAwait(false);
                    count++;
                }
                catch (Exception ex)
                {
                    LogManager.Log(ex, $"Could not delete orphaned cache file: {file}");
                }
            }
            return count;
        }

    }
}