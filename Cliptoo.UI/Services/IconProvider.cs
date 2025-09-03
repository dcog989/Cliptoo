using System;
using System.Collections.Concurrent;
using System.IO;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Services;
using SkiaSharp;
using Svg.Skia;

namespace Cliptoo.UI.Services
{
    public class IconProvider : IIconProvider
    {
        private readonly ConcurrentDictionary<string, ImageSource> _cache = new();
        private readonly ISettingsManager _settingsManager;
        private readonly string _iconCachePath;

        public IconProvider(ISettingsManager settingsManager, string appDataLocalPath)
        {
            _settingsManager = settingsManager;
            _iconCachePath = Path.Combine(appDataLocalPath, "Cliptoo", "IconCache");
            Directory.CreateDirectory(_iconCachePath);
        }
        public async Task<ImageSource?> GetIconAsync(string key, int size = 20)
        {
            if (string.IsNullOrEmpty(key)) return null;

            var dpiScale = GetDpiScale();

            var cacheKey = $"{key}_{size}_{dpiScale}";

            if (int.TryParse(key, out _))
            {
                var settings = _settingsManager.Load();
                cacheKey = $"{key}_{size}_{dpiScale}_{settings.AccentColor}";
            }

            if (_cache.TryGetValue(cacheKey, out var cachedImage))
            {
                LogManager.LogDebug($"ICON_CACHE_DIAG: In-memory hit for key '{cacheKey}'.");
                return cachedImage;
            }

            var cacheFilePath = ServiceUtils.GetCachePath(cacheKey, _iconCachePath, ".png");
            byte[]? iconBytes = null;

            if (File.Exists(cacheFilePath))
            {
                LogManager.LogDebug($"ICON_CACHE_DIAG: On-disk hit for key '{cacheKey}'.");
                try
                {
                    iconBytes = await File.ReadAllBytesAsync(cacheFilePath).ConfigureAwait(false);
                }
                catch (Exception ex)
                {
                    LogManager.Log(ex, $"Failed to read cached icon file: {cacheFilePath}");
                }
            }
            else
            {
                LogManager.LogDebug($"ICON_CACHE_DIAG: Miss for key '{cacheKey}'. Generating new icon.");
                iconBytes = await GenerateIconBytesAsync(key, size, dpiScale).ConfigureAwait(false);
                if (iconBytes != null)
                {
                    try
                    {
                        await File.WriteAllBytesAsync(cacheFilePath, iconBytes).ConfigureAwait(false);
                    }
                    catch (Exception ex)
                    {
                        LogManager.Log(ex, $"Failed to save icon to cache: {cacheFilePath}");
                    }
                }
            }

            if (iconBytes == null) return null;

            return await Application.Current.Dispatcher.InvokeAsync(() =>
            {
                using var ms = new MemoryStream(iconBytes);
                var bitmapImage = new BitmapImage();
                bitmapImage.BeginInit();
                bitmapImage.CacheOption = BitmapCacheOption.OnLoad;
                bitmapImage.StreamSource = ms;
                bitmapImage.EndInit();
                bitmapImage.Freeze();

                var drawingImage = new DrawingImage(new ImageDrawing(bitmapImage, new Rect(0, 0, size, size)));
                drawingImage.Freeze();

                _cache.TryAdd(cacheKey, drawingImage);
                return drawingImage;
            });
        }

        private async Task<byte[]?> GenerateIconBytesAsync(string key, int size, double dpiScale)
        {
            var iconName = GetIconFileName(key);
            var physicalSize = (int)Math.Ceiling(size * dpiScale);

            try
            {
                string? svgContent = await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    var uri = new Uri($"pack://application:,,,/Assets/Icons/{iconName}");
                    var streamInfo = Application.GetResourceStream(uri);
                    if (streamInfo == null) return null;
                    using var stream = streamInfo.Stream;
                    using var reader = new StreamReader(stream);
                    return reader.ReadToEnd();
                });

                if (svgContent == null) return null;

                if (int.TryParse(key, out _))
                {
                    var settings = _settingsManager.Load();
                    var accentColor = (Color)ColorConverter.ConvertFromString(settings.AccentColor);
                    var accentColorHex = $"#{accentColor.R:X2}{accentColor.G:X2}{accentColor.B:X2}";

                    var brightness = (accentColor.R * 299 + accentColor.G * 587 + accentColor.B * 114) / 1000;
                    var textColorHex = brightness > 128 ? "#000000" : "#FFFFFF";

                    svgContent = svgContent.Replace(@"fill=""black""", $"fill=\"{accentColorHex}\"", StringComparison.OrdinalIgnoreCase);
                    svgContent = svgContent.Replace(@"fill=""white""", $"fill=\"{textColorHex}\"", StringComparison.OrdinalIgnoreCase);
                }
                else if (svgContent.Contains("currentColor", StringComparison.OrdinalIgnoreCase))
                {
                    // Create a white stencil for any icon that uses currentColor
                    svgContent = svgContent.Replace("currentColor", "#FFFFFF", StringComparison.OrdinalIgnoreCase);
                }
                else
                {
                    // For multi-color icons like the logo, do nothing and let them render as-is.
                }

                return await Task.Run(() =>
                {
                    using var skSvg = new SKSvg();
                    using var picture = skSvg.FromSvg(svgContent);
                    if (picture == null || picture.CullRect.Width <= 0 || picture.CullRect.Height <= 0) return null;

                    var bitmap = new SKBitmap(physicalSize, physicalSize);
                    using (var canvas = new SKCanvas(bitmap))
                    {
                        canvas.Clear(SKColors.Transparent);
                        float scale = Math.Min(physicalSize / picture.CullRect.Width, physicalSize / picture.CullRect.Height);
                        var matrix = SKMatrix.CreateScale(scale, scale);
                        matrix.TransX = -picture.CullRect.Left * scale;
                        matrix.TransY = -picture.CullRect.Top * scale;
                        using (var paint = new SKPaint { IsAntialias = true })
                        {
                            canvas.DrawPicture(picture, matrix, paint);
                        }
                    }

                    using var image = SKImage.FromBitmap(bitmap);
                    using var data = image.Encode(SKEncodedImageFormat.Png, 100);
                    return data?.ToArray();
                });
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, $"Failed to load icon for key: {key}");
                return null;
            }
        }
        private string GetIconFileName(string key)
        {
            if (int.TryParse(key, out int num) && num >= 1 && num <= 9)
            {
                return $"circle-number-{num}.svg";
            }

            return key.ToLowerInvariant() switch
            {
                Core.AppConstants.ClipTypes.Archive => "file-zip.svg",
                Core.AppConstants.ClipTypes.Audio => "file-music.svg",
                Core.AppConstants.ClipTypes.Dev => "file-code.svg",
                Core.AppConstants.ClipTypes.CodeSnippet => "code.svg",
                Core.AppConstants.FilterKeys.Color => "palette.svg",
                Core.AppConstants.ClipTypes.Danger => "exclamation-diamond.svg",
                Core.AppConstants.ClipTypes.Document => "journal-text.svg",
                Core.AppConstants.ClipTypes.FileText => "file-text.svg",
                Core.AppConstants.ClipTypes.Folder => "folder.svg",
                Core.AppConstants.FilterKeys.Image => "image.svg",
                Core.AppConstants.ClipTypes.Database => "database.svg",
                Core.AppConstants.ClipTypes.Font => "file-font.svg",
                Core.AppConstants.ClipTypes.FileLink => "link-45deg.svg",
                Core.AppConstants.FilterKeys.Link => "link-45deg.svg",
                Core.AppConstants.ClipTypes.System => "microsoft.svg",
                Core.AppConstants.ClipTypes.Rtf => "blockquote.svg",
                Core.AppConstants.FilterKeys.Text => "text-paragraph.svg",
                Core.AppConstants.ClipTypes.Video => "film.svg",
                Core.AppConstants.FilterKeys.All => "check2-all.svg",
                Core.AppConstants.FilterKeys.Pinned => "pin-angle.svg",
                Core.AppConstants.IconKeys.Error => "x-circle.svg",
                Core.AppConstants.IconKeys.List => "list.svg",
                Core.AppConstants.IconKeys.Logo => "cliptoo.svg",
                Core.AppConstants.IconKeys.Multiline => "text-wrap.svg",
                Core.AppConstants.IconKeys.Pin => "pin-angle.svg",
                Core.AppConstants.IconKeys.WasTrimmed => "backspace.svg",
                _ => "file.svg"
            };
        }

        private double GetDpiScale()
        {
            if (Application.Current?.MainWindow != null && VisualTreeHelper.GetDpi(Application.Current.MainWindow).PixelsPerDip != 0)
            {
                return VisualTreeHelper.GetDpi(Application.Current.MainWindow).DpiScaleX;
            }
            return 1.0;
        }

        public int CleanupIconCache()
        {
            try
            {
                var oldFiles = Directory.EnumerateFiles(_iconCachePath, "*.png")
                    .Where(f => (DateTime.UtcNow - new FileInfo(f).CreationTimeUtc) > TimeSpan.FromDays(30));

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
                        LogManager.Log(ex, $"Could not delete old icon cache file: {file}");
                    }
                }
                if (filesDeleted > 0)
                {
                    LogManager.Log($"Cleaned up {filesDeleted} old icon cache files.");
                }
                return filesDeleted;
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, "Failed to perform icon cache cleanup.");
                return 0;
            }
        }

        public void ClearCache()
        {
            try
            {
                ServiceUtils.DeleteDirectoryContents(_iconCachePath);
                _cache.Clear();
                LogManager.Log("Icon cache cleared successfully.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.Log(ex, "Failed to clear icon cache.");
            }
        }
    }
}