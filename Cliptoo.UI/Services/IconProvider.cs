using System.IO;
using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using SixLabors.ImageSharp;

namespace Cliptoo.UI.Services
{
    internal class IconProvider : IIconProvider
    {
        private readonly LruCache<string, ImageSource> _cache;
        private const int MaxCacheSize = 500;
        private readonly ISettingsService _settingsService;
        private readonly string _iconCachePath;

        private static readonly Dictionary<string, string> _iconMap = new(StringComparer.OrdinalIgnoreCase)
        {
            [Core.AppConstants.ClipTypes.Archive] = "file-zip.svg",
            [Core.AppConstants.ClipTypes.Audio] = "file-music.svg",
            [Core.AppConstants.ClipTypes.Dev] = "file-code.svg",
            [Core.AppConstants.ClipTypes.CodeSnippet] = "code.svg",
            [Core.AppConstants.FilterKeys.Color] = "palette.svg",
            [Core.AppConstants.ClipTypes.Danger] = "exclamation-diamond.svg",
            [Core.AppConstants.ClipTypes.Document] = "journal-text.svg",
            [Core.AppConstants.ClipTypes.FileText] = "file-text.svg",
            [Core.AppConstants.ClipTypes.Folder] = "folder.svg",
            [Core.AppConstants.FilterKeys.Image] = "image.svg",
            [Core.AppConstants.ClipTypes.Database] = "database.svg",
            [Core.AppConstants.ClipTypes.Font] = "file-font.svg",
            [Core.AppConstants.ClipTypes.FileLink] = "link-45deg.svg",
            [Core.AppConstants.FilterKeys.Link] = "link-45deg.svg",
            [Core.AppConstants.ClipTypes.System] = "microsoft.svg",
            [Core.AppConstants.ClipTypes.Rtf] = "blockquote.svg",
            [Core.AppConstants.FilterKeys.Text] = "text-paragraph.svg",
            [Core.AppConstants.ClipTypes.Video] = "film.svg",
            [Core.AppConstants.FilterKeys.All] = "check2-all.svg",
            [Core.AppConstants.FilterKeys.Pinned] = "pin-angle.svg",
            [Core.AppConstants.IconKeys.Error] = "x-circle.svg",
            [Core.AppConstants.IconKeys.List] = "list.svg",
            [Core.AppConstants.IconKeys.Logo] = "cliptoo.svg",
            [Core.AppConstants.IconKeys.Multiline] = "text-wrap.svg",
            [Core.AppConstants.IconKeys.Pin] = "pin-angle.svg",
            [Core.AppConstants.IconKeys.WasTrimmed] = "backspace.svg",
            [Core.AppConstants.IconKeys.Trash] = "trash.svg"
        };

        public IconProvider(ISettingsService settingsService, string appDataLocalPath)
        {
            _settingsService = settingsService;
            _iconCachePath = Path.Combine(appDataLocalPath, "Cliptoo", "IconCache");
            Directory.CreateDirectory(_iconCachePath);
            _cache = new LruCache<string, ImageSource>(MaxCacheSize);
        }
        public async Task<ImageSource?> GetIconAsync(string key, int size = 20)
        {
            if (string.IsNullOrEmpty(key)) return null;

            var dpiScale = GetDpiScale();

            var cacheKey = $"{key}_{size}_{dpiScale}";

            if (int.TryParse(key, out _))
            {
                var settings = _settingsService.Settings;
                cacheKey = $"{key}_{size}_{dpiScale}_{settings.AccentColor}";
            }

            if (_cache.TryGetValue(cacheKey, out var cachedImage) && cachedImage != null)
            {
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
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                {
                    LogManager.LogWarning($"Failed to read cached icon file: {cacheFilePath}. Error: {ex.Message}");
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
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
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