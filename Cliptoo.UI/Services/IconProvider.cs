using System.IO;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using SixLabors.ImageSharp;
using System.Collections.Concurrent;
using System.Globalization;
using System.Windows.Resources;

namespace Cliptoo.UI.Services
{
    internal class IconProvider : IIconProvider
    {
        private readonly LruCache<string, ImageSource> _cache;
        private const int MaxCacheSize = 500;
        private readonly ISettingsService _settingsService;
        private readonly string _iconCachePath;
        private readonly ConcurrentDictionary<string, Task<ImageSource?>> _ongoingGenerations = new();

        private static readonly Dictionary<string, string> _iconMap = new(StringComparer.OrdinalIgnoreCase)
        {
            [Core.AppConstants.ClipTypeArchive] = "file-zip.svg",
            [Core.AppConstants.ClipTypeAudio] = "file-music.svg",
            [Core.AppConstants.ClipTypeCodeSnippet] = "code.svg",
            [Core.AppConstants.ClipTypeDanger] = "exclamation-diamond.svg",
            [Core.AppConstants.ClipTypeDatabase] = "database.svg",
            [Core.AppConstants.ClipTypeDev] = "file-code.svg",
            [Core.AppConstants.ClipTypeDocument] = "journal-text.svg",
            [Core.AppConstants.ClipTypeFileLink] = "link-45deg.svg",
            [Core.AppConstants.ClipTypeFileText] = "file-text.svg",
            [Core.AppConstants.ClipTypeFolder] = "folder.svg",
            [Core.AppConstants.ClipTypeFont] = "file-font.svg",
            [Core.AppConstants.ClipTypeRtf] = "blockquote.svg",
            [Core.AppConstants.ClipTypeSystem] = "microsoft.svg",
            [Core.AppConstants.ClipTypeVideo] = "film.svg",
            [Core.AppConstants.FilterKeyAll] = "check2-all.svg",
            [Core.AppConstants.FilterKeyColor] = "palette.svg",
            [Core.AppConstants.FilterKeyImage] = "image.svg",
            [Core.AppConstants.FilterKeyLink] = "link-45deg.svg",
            [Core.AppConstants.FilterKeyText] = "text-paragraph.svg",
            [Core.AppConstants.IconKeyError] = "x-circle.svg",
            [Core.AppConstants.IconKeyFavorite] = "star.svg",
            [Core.AppConstants.IconKeyList] = "list.svg",
            [Core.AppConstants.IconKeyLogo] = "cliptoo.svg",
            [Core.AppConstants.IconKeyMultiline] = "text-wrap.svg",
            [Core.AppConstants.IconKeyTrash] = "trash.svg",
            [Core.AppConstants.IconKeyWasTrimmed] = "backspace.svg"
        };

        public IconProvider(ISettingsService settingsService, string appDataLocalPath)
        {
            _settingsService = settingsService;
            _iconCachePath = Path.Combine(appDataLocalPath, "Cliptoo", "IconCache");
            Directory.CreateDirectory(_iconCachePath);
            _cache = new LruCache<string, ImageSource>(MaxCacheSize);
        }
        public Task<ImageSource?> GetIconAsync(string key, int size = 20)
        {
            if (string.IsNullOrEmpty(key)) return Task.FromResult<ImageSource?>(null);

            var dpiScale = GetDpiScale();

            var cacheKey = $"{key}_{size}_{dpiScale.ToString(CultureInfo.InvariantCulture)}";

            if (int.TryParse(key, out _))
            {
                var settings = _settingsService.Settings;
                cacheKey = $"{key}_{size}_{dpiScale.ToString(CultureInfo.InvariantCulture)}_{settings.AccentColor}";
            }

            if (_cache.TryGetValue(cacheKey, out var cachedImage) && cachedImage != null)
            {
                return Task.FromResult<ImageSource?>(cachedImage);
            }

            return _ongoingGenerations.GetOrAdd(cacheKey, GenerateAndCacheIconAsync);
        }

        private async Task<ImageSource?> GenerateAndCacheIconAsync(string cacheKey)
        {
            try
            {
                var cacheFilePath = ServiceUtils.GetCachePath(cacheKey, _iconCachePath, ".png");
                byte[]? iconBytes = null;

                if (File.Exists(cacheFilePath))
                {
                    LogManager.LogDebug($"ICON_CACHE_DIAG: On-disk hit for key '{cacheKey}'.");
                    try
                    {
                        iconBytes = await File.ReadAllBytesAsync(cacheFilePath).ConfigureAwait(false);
                    }
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                    {
                        LogManager.LogWarning($"Failed to read cached icon file: {cacheFilePath}. Error: {ex.Message}");
                    }
                }
                else
                {
                    LogManager.LogDebug($"ICON_CACHE_DIAG: Miss for key '{cacheKey}'. Generating new icon.");

                    var parts = cacheKey.Split('_');
                    string key;
                    int size;
                    double dpiScale;

                    if (parts.Last().StartsWith('#'))
                    {
                        // Format: {key}_{size}_{dpiScale}_{accentColor}
                        if (parts.Length < 4) throw new ArgumentException($"Invalid cacheKey format for accent color icon: {cacheKey}", nameof(cacheKey));
                        dpiScale = double.Parse(parts[parts.Length - 2], CultureInfo.InvariantCulture);
                        size = int.Parse(parts[parts.Length - 3], CultureInfo.InvariantCulture);
                        key = string.Join("_", parts.Take(parts.Length - 3));
                    }
                    else
                    {
                        // Format: {key}_{size}_{dpiScale}
                        if (parts.Length < 3) throw new ArgumentException($"Invalid cacheKey format for standard icon: {cacheKey}", nameof(cacheKey));
                        dpiScale = double.Parse(parts.Last(), CultureInfo.InvariantCulture);
                        size = int.Parse(parts[parts.Length - 2], CultureInfo.InvariantCulture);
                        key = string.Join("_", parts.Take(parts.Length - 2));
                    }


                    iconBytes = await GenerateIconBytesAsync(key, size, dpiScale).ConfigureAwait(false);
                    if (iconBytes != null)
                    {
                        try
                        {
                            await File.WriteAllBytesAsync(cacheFilePath, iconBytes).ConfigureAwait(false);
                        }
                        catch (IOException ex)
                        {
                            LogManager.LogDebug($"Could not write icon cache file (may be a harmless race condition): {cacheFilePath}. Error: {ex.Message}");
                        }
                        catch (Exception ex) when (ex is UnauthorizedAccessException)
                        {
                            LogManager.LogWarning($"Failed to save icon to cache: {cacheFilePath}. Error: {ex.Message}");
                        }
                    }
                }

                if (iconBytes == null) return null;

                var bitmapImage = new BitmapImage();
                using (var ms = new MemoryStream(iconBytes))
                {
                    bitmapImage.BeginInit();
                    bitmapImage.StreamSource = ms;
                    bitmapImage.CacheOption = BitmapCacheOption.OnLoad;
                    bitmapImage.EndInit();
                }
                bitmapImage.Freeze();
                _cache.Add(cacheKey, bitmapImage);
                return bitmapImage;
            }
            finally
            {
                _ongoingGenerations.TryRemove(cacheKey, out _);
            }
        }

        private async Task<byte[]?> GenerateIconBytesAsync(string key, int size, double dpiScale)
        {
            var iconName = GetIconFileName(key);
            var physicalSize = (int)Math.Ceiling(size * dpiScale);

            try
            {
                StreamResourceInfo? streamInfo = await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    var uri = new Uri($"pack://application:,,,/Assets/Icons/{iconName}");
                    return Application.GetResourceStream(uri);
                });

                if (streamInfo is null)
                {
                    return null;
                }

                string svgContent;
                using (var stream = streamInfo.Stream)
                using (var reader = new StreamReader(stream))
                {
                    svgContent = await reader.ReadToEndAsync().ConfigureAwait(false);
                }

                if (string.IsNullOrEmpty(svgContent))
                {
                    return null;
                }

                if (int.TryParse(key, out _))
                {
                    var settings = _settingsService.Settings;
                    var accentColor = (System.Windows.Media.Color)ColorConverter.ConvertFromString(settings.AccentColor);
                    var accentColorHex = $"#{accentColor.R:X2}{accentColor.G:X2}{accentColor.B:X2}";

                    var brightness = (accentColor.R * 299 + accentColor.G * 587 + accentColor.B * 114) / 1000;
                    var textColorHex = brightness > 128 ? "#000000" : "#FFFFFF";

                    svgContent = svgContent.Replace(@"fill=""black""", $"fill=\"{accentColorHex}\"", StringComparison.OrdinalIgnoreCase);
                    svgContent = svgContent.Replace(@"fill=""white""", $"fill=\"{textColorHex}\"", StringComparison.OrdinalIgnoreCase);
                }
                else if (svgContent.Contains("currentColor", StringComparison.OrdinalIgnoreCase))
                {
                    svgContent = svgContent.Replace("currentColor", "#FFFFFF", StringComparison.OrdinalIgnoreCase);
                }

                using var imageSharpImage = await Task.Run(() => ServiceUtils.RenderSvgToImageSharp(svgContent, physicalSize)).ConfigureAwait(false);
                if (imageSharpImage == null) return null;

                using var ms = new MemoryStream();
                await imageSharpImage.SaveAsPngAsync(ms).ConfigureAwait(false);
                return ms.ToArray();
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Xml.XmlException)
            {
                LogManager.LogCritical(ex, $"Failed to load icon for key: {key}");
                return null;
            }
        }

        private static string GetIconFileName(string key)
        {
            if (int.TryParse(key, out int num) && num >= 1 && num <= 9)
            {
                return $"circle-number-{num}.svg";
            }

            if (_iconMap.TryGetValue(key, out var iconName))
            {
                return iconName;
            }

            return "file.svg";
        }

        private static double GetDpiScale()
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
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                    {
                        LogManager.LogWarning($"Could not delete old icon cache file: {file}. Error: {ex.Message}");
                    }
                }
                if (filesDeleted > 0)
                {
                    LogManager.LogInfo($"Cleaned up {filesDeleted} old icon cache files.");
                }
                return filesDeleted;
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.LogCritical(ex, "Failed to perform icon cache cleanup.");
                return 0;
            }
        }

        public void ClearCache()
        {
            try
            {
                ServiceUtils.DeleteDirectoryContents(_iconCachePath);
                _cache.Clear();
                LogManager.LogInfo("Icon cache cleared successfully.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.LogCritical(ex, "Failed to clear icon cache.");
            }
        }

    }
}