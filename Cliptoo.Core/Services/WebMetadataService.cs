using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Configuration;
using SixLabors.ImageSharp;
using SixLabors.ImageSharp.Formats.Png;
using SixLabors.ImageSharp.Processing;

namespace Cliptoo.Core.Services
{
    public class WebMetadataService : IWebMetadataService, IDisposable
    {
        private const int ThumbnailSize = 32;
        private static readonly TimeSpan FailureCacheDuration = TimeSpan.FromDays(7);
        private readonly string _faviconCacheDir;
        private readonly HttpClient _httpClient;
        private readonly PngEncoder _pngEncoder;
        private static readonly Regex LinkTagRegex = new("<link[^>]+>", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex RelAttributeRegex = new("rel\\s*=\\s*(?:['\"](?<v>[^'\"]*)['\"]|(?<v>[^>\\s]+))", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex HrefAttributeRegex = new("href\\s*=\\s*(?:['\"]([^'\"]+)['\"]|([^>\\s]+))", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex SizesAttributeRegex = new("sizes\\s*=\\s*(?:['\"](?<v>[^'\"]*)['\"]|(?<v>[^>\\s]+))", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex TitleRegex = new("<title[^>]*>\\s*(.+?)\\s*</title>", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private readonly LruCache<string, string> _titleCache;
        private readonly ConcurrentDictionary<string, bool> _failedFaviconUrls = new();
        private bool _disposedValue;
        private readonly IImageDecoder _imageDecoder;

        public WebMetadataService(string appCachePath, IImageDecoder imageDecoder)
        {
            _faviconCacheDir = Path.Combine(appCachePath, "Cliptoo", "FaviconCache");
            Directory.CreateDirectory(_faviconCacheDir);

            _httpClient = new HttpClient();
            _httpClient.DefaultRequestHeaders.UserAgent.ParseAdd("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36");
            _httpClient.Timeout = TimeSpan.FromSeconds(5);

            _pngEncoder = new PngEncoder { CompressionLevel = PngCompressionLevel.Level6 };
            _titleCache = new LruCache<string, string>(100);
            _imageDecoder = imageDecoder;
        }

        private struct FaviconCandidate
        {
            public string Url { get; set; }
            public int Score { get; set; }
        }

        public async Task<string?> GetFaviconAsync(Uri url)
        {
            if (url is null) return null;
            var urlString = url.GetLeftPart(UriPartial.Authority);
            if (_failedFaviconUrls.ContainsKey(urlString))
            {
                LogManager.LogDebug($"FAVICON_CACHE_DIAG: Hit (failure cache) for '{urlString}'.");
                return null;
            }

            var successCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".png");
            if (File.Exists(successCachePath))
            {
                LogManager.LogDebug($"FAVICON_CACHE_DIAG: Hit for '{urlString}'.");
                return successCachePath;
            }

            var failureCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".failed");
            if (File.Exists(failureCachePath))
            {
                try
                {
                    var timestampText = await File.ReadAllTextAsync(failureCachePath).ConfigureAwait(false);
                    if (DateTime.TryParse(timestampText, null, System.Globalization.DateTimeStyles.RoundtripKind, out var timestamp))
                    {
                        if ((DateTime.UtcNow - timestamp) < FailureCacheDuration)
                        {
                            _failedFaviconUrls.TryAdd(urlString, true);
                            return null;
                        }
                    }
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                {
                    LogManager.Log(ex, $"Failed to read failure cache file for {urlString}");
                }
            }

            LogManager.LogDebug($"FAVICON_CACHE_DIAG: Miss for '{urlString}'. Starting fetch process.");
            if (url.Scheme == "data")
            {
                return await ProcessDataUriFaviconAsync(urlString, successCachePath).ConfigureAwait(false);
            }

            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            string? finalIconPath = null;
            try
            {
                LogManager.LogDebug($"Favicon Discovery for {urlString}: Stage 1 (HTML Head Parse).");
                finalIconPath = await TryFetchIconsFromHtmlAsync(url, successCachePath).ConfigureAwait(false);

                if (finalIconPath == null)
                {
                    LogManager.LogDebug($"Favicon Discovery for {urlString}: Stage 1 failed. Stage 2 (Root Icon Check).");
                    finalIconPath = await TryFetchRootIconAsync(url, successCachePath).ConfigureAwait(false);
                }

                if (finalIconPath != null)
                {
                    if (File.Exists(failureCachePath))
                    {
                        try
                        {
                            File.Delete(failureCachePath);
                        }
                        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                        {
                            LogManager.Log(ex, $"Could not delete stale failure cache file: {failureCachePath}");
                        }
                    }
                }
                else
                {
                    LogManager.LogDebug($"Favicon Discovery for {urlString}: All stages failed. Caching failure.");
                    _failedFaviconUrls.TryAdd(urlString, true);
                    try
                    {
                        await File.WriteAllTextAsync(failureCachePath, DateTime.UtcNow.ToString("o")).ConfigureAwait(false);
                    }
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                    {
                        LogManager.Log(ex, $"Failed to create/update failure cache file for {urlString}");
                    }
                }
            }
            finally
            {
                stopwatch.Stop();
                LogManager.LogDebug($"PERF_DIAG: Favicon discovery for '{urlString}' took {stopwatch.ElapsedMilliseconds}ms.");
            }

            return finalIconPath;
        }

        private async Task<string?> TryFetchRootIconAsync(Uri baseUri, string cachePath)
        {
            var rootIconNames = new[] { "/favicon.ico", "/favicon.png", "/favicon.svg" };
            using var cts = new CancellationTokenSource();
            using var timeoutCts = new CancellationTokenSource(TimeSpan.FromSeconds(2));
            using var linkedCts = CancellationTokenSource.CreateLinkedTokenSource(cts.Token, timeoutCts.Token);

            var tasks = rootIconNames.Select(iconName =>
            {
                var faviconUrl = new Uri(baseUri, iconName);
                return FetchAndProcessFavicon(faviconUrl.ToString(), cachePath, linkedCts.Token);
            }).ToList();

            while (tasks.Count > 0)
            {
                var completedTask = await Task.WhenAny(tasks).ConfigureAwait(false);
                tasks.Remove(completedTask);

                if (await completedTask.ConfigureAwait(false))
                {
                    await cts.CancelAsync().ConfigureAwait(false); // Cancel remaining tasks
                    return cachePath;
                }
            }
            return null;
        }

        private async Task<string?> TryFetchIconsFromHtmlAsync(Uri pageUri, string cachePath)
        {
            string? headContent;
            try
            {
                var response = await _httpClient.GetAsync(pageUri, HttpCompletionOption.ResponseHeadersRead).ConfigureAwait(false);
                if (!response.IsSuccessStatusCode) return null;

                Stream stream = await response.Content.ReadAsStreamAsync().ConfigureAwait(false);
                try
                {
                    headContent = await ReadHeadContentAsync(stream).ConfigureAwait(false);
                }
                finally
                {
                    await stream.DisposeAsync().ConfigureAwait(false);
                }
            }
            catch (HttpRequestException ex)
            {
                LogManager.LogDebug($"Failed to fetch HTML head for {pageUri}: {ex.Message}");
                return null;
            }
            catch (TaskCanceledException)
            {
                LogManager.LogDebug($"HTML head fetch for {pageUri} was canceled.");
                return null;
            }

            if (string.IsNullOrEmpty(headContent)) return null;
            LogManager.LogDebug($"FAVICON_DISCOVERY_DIAG: HTML head content length: {headContent.Length}.");

            var iconCandidates = new List<FaviconCandidate>();
            foreach (Match linkMatch in LinkTagRegex.Matches(headContent))
            {
                var linkTag = linkMatch.Value;
                var relMatch = RelAttributeRegex.Match(linkTag);
                if (!relMatch.Success) continue;

                var relValue = relMatch.Groups["v"].Value;
                var relParts = relValue.Split(new[] { ' ' }, StringSplitOptions.RemoveEmptyEntries);
                var isIcon = relParts.Any(p =>
                    p.Equals("icon", StringComparison.OrdinalIgnoreCase) ||
                    p.Equals("shortcut", StringComparison.OrdinalIgnoreCase) ||
                    p.Equals("apple-touch-icon", StringComparison.OrdinalIgnoreCase) ||
                    p.Equals("apple-touch-icon-precomposed", StringComparison.OrdinalIgnoreCase)
                );

                if (!isIcon)
                {
                    continue;
                }

                var hrefMatch = HrefAttributeRegex.Match(linkTag);
                if (!hrefMatch.Success) continue;

                var href = hrefMatch.Groups[1].Success ? hrefMatch.Groups[1].Value : hrefMatch.Groups[2].Value;
                if (string.IsNullOrWhiteSpace(href)) continue;

                string fullUrl;
                if (href.StartsWith("//", StringComparison.Ordinal))
                {
                    fullUrl = $"{pageUri.Scheme}:{href}";
                }
                else
                {
                    fullUrl = new Uri(pageUri, href).ToString();
                }

                int score = 0;
                string extension = Path.GetExtension(fullUrl).ToLowerInvariant();

                if (extension == ".svg")
                {
                    score = 10000;
                }
                else if (extension == ".ico")
                {
                    score = 5000;
                }
                else
                {
                    var sizesMatch = SizesAttributeRegex.Match(linkTag);
                    if (sizesMatch.Success)
                    {
                        var sizesValue = sizesMatch.Groups["v"].Value.ToLowerInvariant();
                        if (sizesValue == "any")
                        {
                            score = 100;
                        }
                        else
                        {
                            var firstSizePart = sizesValue.Split(' ')[0];
                            var dimension = firstSizePart.Split('x')[0];
                            if (int.TryParse(dimension, out int size))
                            {
                                if (size >= 32)
                                {
                                    score = 1000 - (size - 32);
                                }
                                else
                                {
                                    score = size;
                                }
                            }
                        }
                    }
                    else
                    {
                        score = 32;
                    }
                }

                iconCandidates.Add(new FaviconCandidate { Url = fullUrl, Score = score });
            }

            if (iconCandidates.Count == 0)
            {
                LogManager.LogDebug("FAVICON_DISCOVERY_DIAG: No icon URLs found in the HTML head.");
                return null;
            }

            var orderedUrls = iconCandidates.OrderByDescending(c => c.Score).Select(c => c.Url).Distinct().ToList();

            LogManager.LogDebug($"FAVICON_DISCOVERY_DIAG: Found {orderedUrls.Count} potential icon URLs. Best candidate: '{orderedUrls.FirstOrDefault()}'");

            using var cts = new CancellationTokenSource();
            using var timeoutCts = new CancellationTokenSource(TimeSpan.FromSeconds(3));
            using var linkedCts = CancellationTokenSource.CreateLinkedTokenSource(cts.Token, timeoutCts.Token);

            var tasks = orderedUrls.Select(url => FetchAndProcessFavicon(url, cachePath, linkedCts.Token)).ToList();

            while (tasks.Count > 0)
            {
                var completedTask = await Task.WhenAny(tasks).ConfigureAwait(false);
                tasks.Remove(completedTask);

                if (await completedTask.ConfigureAwait(false))
                {
                    await cts.CancelAsync().ConfigureAwait(false);
                    return cachePath;
                }
            }

            return null;
        }

        private static async Task<string?> ReadHeadContentAsync(Stream stream)
        {
            using var reader = new StreamReader(stream, Encoding.UTF8, true, 1024, true);
            var buffer = new char[150 * 1024]; // Increased buffer to 150KB

            int charsRead = await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false);
            if (charsRead == 0) return null;

            var content = new string(buffer, 0, charsRead);
            int headEndIndex = content.IndexOf("</head>", StringComparison.OrdinalIgnoreCase);

            if (headEndIndex != -1)
            {
                // Return only the content up to the end of the head tag
                return content.Substring(0, headEndIndex + "</head>".Length);
            }

            LogManager.LogDebug("HTML </head> tag not found within the first 150KB. Using partial content for discovery.");
            return content;
        }

        public async Task<string?> GetPageTitleAsync(Uri url)
        {
            ArgumentNullException.ThrowIfNull(url);
            var urlString = url.ToString();
            if (_titleCache.TryGetValue(urlString, out var cachedTitle))
            {
                LogManager.LogDebug($"TITLE_CACHE_DIAG: Hit for '{urlString}'.");
                return string.IsNullOrEmpty(cachedTitle) ? null : cachedTitle;
            }
            LogManager.LogDebug($"TITLE_CACHE_DIAG: Miss for '{urlString}'. Fetching from web.");

            if (url.Scheme != Uri.UriSchemeHttp && url.Scheme != Uri.UriSchemeHttps)
            {
                _titleCache.Add(urlString, string.Empty);
                return null;
            }

            try
            {
                using var response = await _httpClient.GetAsync(url, HttpCompletionOption.ResponseHeadersRead).ConfigureAwait(false);
                if (!response.IsSuccessStatusCode)
                {
                    _titleCache.Add(urlString, string.Empty);
                    return null;
                }

                var encoding = Encoding.UTF8;
                var charset = response.Content.Headers.ContentType?.CharSet;
                if (!string.IsNullOrEmpty(charset))
                {
                    try
                    {
                        encoding = Encoding.GetEncoding(charset);
                    }
                    catch (ArgumentException) { /* Fallback to UTF-8 */ }
                }

                var stream = await response.Content.ReadAsStreamAsync().ConfigureAwait(false);
                try
                {
                    using var reader = new StreamReader(stream, encoding, true, 1024, true); // leaveOpen = true

                    var buffer = new char[4096];
                    var content = new StringBuilder();
                    string? title = null;

                    while (true)
                    {
                        int charsRead = await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false);
                        if (charsRead == 0) break;

                        content.Append(buffer, 0, charsRead);
                        var currentContent = content.ToString();

                        var match = TitleRegex.Match(currentContent);
                        if (match.Success)
                        {
                            title = System.Net.WebUtility.HtmlDecode(match.Groups[1].Value.Trim());
                            break;
                        }

                        if (currentContent.Contains("</head>", StringComparison.OrdinalIgnoreCase) || content.Length > 150 * 1024)
                        {
                            break;
                        }
                    }
                    _titleCache.Add(urlString, title ?? string.Empty);
                    return title;
                }
                finally
                {
                    await stream.DisposeAsync().ConfigureAwait(false);
                }
            }
            catch (HttpRequestException ex)
            {
                LogManager.LogDebug($"HTTP request for title failed for {url}: {ex.Message}");
            }
            catch (Exception ex) when (ex is TaskCanceledException or IOException)
            {
                LogManager.Log(ex, $"Failed to fetch page title for {url}");
            }

            _titleCache.Add(urlString, string.Empty);
            return null;
        }

        private async Task<bool> FetchAndProcessFavicon(string faviconUrl, string cachePath, CancellationToken cancellationToken = default)
        {
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            try
            {
                LogManager.LogDebug($"Attempting to fetch favicon: {faviconUrl}");

                if (faviconUrl.StartsWith("data:", StringComparison.OrdinalIgnoreCase))
                {
                    var result = await ProcessDataUriFaviconAsync(faviconUrl, cachePath).ConfigureAwait(false);
                    return !string.IsNullOrEmpty(result);
                }

                if (!Uri.TryCreate(faviconUrl, UriKind.Absolute, out var faviconUri)) return false;

                if (faviconUri.Scheme != Uri.UriSchemeHttp && faviconUri.Scheme != Uri.UriSchemeHttps)
                {
                    LogManager.LogDebug($"Unsupported URI scheme '{faviconUri.Scheme}' for favicon discovery. Skipping.");
                    return false;
                }

                var response = await _httpClient.GetAsync(faviconUri, cancellationToken).ConfigureAwait(false);

                if (!response.IsSuccessStatusCode)
                {
                    LogManager.LogDebug($"Favicon fetch failed for {faviconUrl} with status code {response.StatusCode}.");
                    return false;
                }

                var contentType = response.Content.Headers.ContentType?.MediaType;
                LogManager.LogDebug($"Received response for {faviconUrl}. Status: {response.StatusCode}, Content-Type: {contentType}");

                byte[]? outputBytes = null;
                var uri = new Uri(faviconUrl);
                var extension = Path.GetExtension(uri.AbsolutePath).ToUpperInvariant();

                if (extension == ".SVG")
                {
                    if (contentType?.Contains("svg", StringComparison.Ordinal) != true)
                    {
                        LogManager.LogDebug($"Expected SVG content-type for {faviconUrl}, but received {contentType}. Skipping parse.");
                        return false;
                    }
                    var svgContent = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
                    outputBytes = await ServiceUtils.GenerateSvgPreviewAsync(svgContent, ThumbnailSize, "light", true).ConfigureAwait(false);
                }
                else
                {
                    using var contentStream = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
                    using var memoryStream = new MemoryStream();
                    await contentStream.CopyToAsync(memoryStream, cancellationToken).ConfigureAwait(false);
                    memoryStream.Position = 0;

                    using var image = await _imageDecoder.DecodeAsync(memoryStream, extension).ConfigureAwait(false);
                    if (image != null)
                    {
                        image.Mutate(x => x.Resize(ThumbnailSize, ThumbnailSize));
                        using var ms = new MemoryStream();
                        await image.SaveAsPngAsync(ms, _pngEncoder, cancellationToken).ConfigureAwait(false);
                        outputBytes = ms.ToArray();
                    }
                }

                if (outputBytes != null)
                {
                    await File.WriteAllBytesAsync(cachePath, outputBytes, cancellationToken).ConfigureAwait(false);
                    LogManager.LogDebug($"Successfully processed and cached favicon for {faviconUrl}.");
                    return true;
                }
            }
            catch (HttpRequestException ex)
            {
                LogManager.LogDebug($"Favicon network request failed for {faviconUrl}: {ex.Message}");
            }
            catch (NotSupportedException ex)
            {
                LogManager.LogDebug($"Favicon URI scheme not supported for {faviconUrl}: {ex.Message}");
            }
            catch (TaskCanceledException)
            {
                LogManager.LogDebug($"Favicon fetch for {faviconUrl} was cancelled (another task likely succeeded).");
            }
            catch (ImageFormatException ex)
            {
                LogManager.Log(ex, $"Favicon fetch/process failed for {faviconUrl}.");
            }
            finally
            {
                stopwatch.Stop();
                LogManager.LogDebug($"PERF_DIAG: Individual favicon fetch for '{faviconUrl}' completed in {stopwatch.ElapsedMilliseconds}ms.");
            }
            return false;
        }

        private async Task<string?> ProcessDataUriFaviconAsync(string dataUri, string cachePath)
        {
            try
            {
                var match = Regex.Match(dataUri, @"^data:(?<type>[^;]+);(?<encoding>\w+),(?<data>.+)$");
                if (!match.Success)
                {
                    LogManager.LogDebug($"Could not parse data URI: {dataUri.Substring(0, Math.Min(100, dataUri.Length))}");
                    return null;
                }

                var encoding = match.Groups["encoding"].Value;
                if (!encoding.Equals("base64", StringComparison.OrdinalIgnoreCase))
                {
                    LogManager.LogDebug($"Unsupported data URI encoding: {encoding}");
                    return null;
                }

                var base64Data = match.Groups["data"].Value;
                var bytes = Convert.FromBase64String(base64Data);
                var mediaType = match.Groups["type"].Value;
                var extension = GetExtensionFromMediaType(mediaType);
                if (string.IsNullOrEmpty(extension))
                {
                    LogManager.LogDebug($"Unsupported media type in data URI: {mediaType}");
                    return null;
                }

                using var stream = new MemoryStream(bytes);
                byte[]? outputBytes = null;

                if (extension == ".SVG")
                {
                    using var reader = new StreamReader(stream);
                    var svgContent = await reader.ReadToEndAsync().ConfigureAwait(false);
                    outputBytes = await ServiceUtils.GenerateSvgPreviewAsync(svgContent, ThumbnailSize, "light", true).ConfigureAwait(false);
                }
                else
                {
                    using var image = await _imageDecoder.DecodeAsync(stream, extension).ConfigureAwait(false);
                    if (image != null)
                    {
                        image.Mutate(x => x.Resize(ThumbnailSize, ThumbnailSize));
                        using var ms = new MemoryStream();
                        await image.SaveAsPngAsync(ms, _pngEncoder).ConfigureAwait(false);
                        outputBytes = ms.ToArray();
                    }
                }

                if (outputBytes != null)
                {
                    await File.WriteAllBytesAsync(cachePath, outputBytes).ConfigureAwait(false);
                    LogManager.LogDebug($"Successfully processed and cached data URI favicon.");
                    return cachePath;
                }
            }
            catch (Exception ex) when (ex is FormatException or ArgumentException)
            {
                LogManager.LogDebug($"Could not process data URI, it may be invalid or truncated. URI: {dataUri.Substring(0, Math.Min(100, dataUri.Length))}. Error: {ex.Message}");
            }
            return null;
        }

        private static string GetExtensionFromMediaType(string mediaType)
        {
            if (mediaType.Contains("svg", StringComparison.OrdinalIgnoreCase)) return ".SVG";
            if (mediaType.Contains("png", StringComparison.OrdinalIgnoreCase)) return ".PNG";
            if (mediaType.Contains("vnd.microsoft.icon", StringComparison.OrdinalIgnoreCase) || mediaType.Contains("x-icon", StringComparison.OrdinalIgnoreCase)) return ".ICO";
            if (mediaType.Contains("webp", StringComparison.OrdinalIgnoreCase)) return ".WEBP";
            if (mediaType.Contains("jpeg", StringComparison.OrdinalIgnoreCase) || mediaType.Contains("jpg", StringComparison.OrdinalIgnoreCase)) return ".JPG";
            return string.Empty;
        }

        public void ClearCache()
        {
            try
            {
                ServiceUtils.DeleteDirectoryContents(_faviconCacheDir);
                _titleCache.Clear();
                _failedFaviconUrls.Clear();
                LogManager.Log("Web metadata caches cleared successfully.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.Log(ex, "Failed to clear web caches.");
            }
        }

        public void ClearCacheForUrl(Uri url)
        {
            if (url is null) return;
            var urlString = url.GetLeftPart(UriPartial.Authority);
            LogManager.LogDebug($"FAVICON_CACHE_DIAG: Clearing cache for URL: {urlString}");

            try
            {
                var successCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".png");
                if (File.Exists(successCachePath))
                {
                    File.Delete(successCachePath);
                    LogManager.LogDebug($"FAVICON_CACHE_DIAG: Deleted success cache file: {successCachePath}");
                }

                var failureCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".failed");
                if (File.Exists(failureCachePath))
                {
                    File.Delete(failureCachePath);
                    LogManager.LogDebug($"FAVICON_CACHE_DIAG: Deleted failure cache file: {failureCachePath}");
                }

                if (_failedFaviconUrls.TryRemove(urlString, out _))
                {
                    LogManager.LogDebug($"FAVICON_CACHE_DIAG: Removed '{urlString}' from in-memory failure cache.");
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.Log(ex, $"Failed to clear cache for URL: {urlString}");
            }
        }

        public async Task<int> PruneCacheAsync()
        {
            int count = 0;
            if (!Directory.Exists(_faviconCacheDir)) return 0;

            var filesToDelete = new List<string>();
            var cutoff = DateTime.UtcNow.AddDays(-90);

            try
            {
                foreach (var file in Directory.EnumerateFiles(_faviconCacheDir))
                {
                    var fileInfo = new FileInfo(file);
                    if (fileInfo.LastAccessTimeUtc < cutoff)
                    {
                        filesToDelete.Add(file);
                    }
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or DirectoryNotFoundException)
            {
                LogManager.Log(ex, $"Failed to enumerate files for pruning in {_faviconCacheDir}");
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
                    LogManager.Log(ex, $"Could not delete orphaned cache file: {file}");
                }
            }
            return count;
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _httpClient.Dispose();
                }
                _disposedValue = true;
            }
        }

        public void Dispose()
        {
            Dispose(disposing: true);
            GC.SuppressFinalize(this);
        }

    }
}