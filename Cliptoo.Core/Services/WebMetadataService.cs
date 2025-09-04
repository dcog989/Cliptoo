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
        private static readonly char[] _spaceSeparator = [' '];

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
                return null;
            }

            var successCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".png");
            if (File.Exists(successCachePath))
            {
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

            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            string? finalIconPath = null;
            try
            {
                var (pageTitle, htmlCandidates) = await FetchAndParseHtmlHeadAsync(url).ConfigureAwait(false);
                if (pageTitle != null)
                {
                    _titleCache.Add(url.ToString(), pageTitle);
                }

                var rootCandidates = new List<FaviconCandidate>
        {
            new() { Url = new Uri(url, "/favicon.svg").ToString(), Score = 9000 },
            new() { Url = new Uri(url, "/favicon.ico").ToString(), Score = 4000 },
            new() { Url = new Uri(url, "/apple-touch-icon.png").ToString(), Score = 3000 },
            new() { Url = new Uri(url, "/favicon-32x32.png").ToString(), Score = 968 },
            new() { Url = new Uri(url, "/favicon.png").ToString(), Score = 500 }
        };

                var allCandidates = htmlCandidates.Concat(rootCandidates)
                    .OrderByDescending(c => c.Score)
                    .Select(c => c.Url)
                    .Distinct()
                    .ToList();

                LogManager.LogDebug($"FAVICON_DISCOVERY_DIAG: Found {allCandidates.Count} total candidates. Best candidate: '{allCandidates.FirstOrDefault()}'");

                using var timeoutCts = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                foreach (var iconUrl in allCandidates)
                {
                    if (await FetchAndProcessFavicon(iconUrl, successCachePath, timeoutCts.Token).ConfigureAwait(false))
                    {
                        finalIconPath = successCachePath;
                        break;
                    }
                    if (timeoutCts.IsCancellationRequested)
                    {
                        LogManager.LogDebug("FAVICON_DISCOVERY_DIAG: Favicon discovery timed out.");
                        break;
                    }
                }

                if (finalIconPath != null)
                {
                    if (File.Exists(failureCachePath))
                    {
                        try { File.Delete(failureCachePath); }
                        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                        { LogManager.Log(ex, $"Could not delete stale failure cache file: {failureCachePath}"); }
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
                    { LogManager.Log(ex, $"Failed to create/update failure cache file for {urlString}"); }
                }
            }
            finally
            {
                stopwatch.Stop();
                LogManager.LogDebug($"PERF_DIAG: Favicon discovery for '{urlString}' took {stopwatch.ElapsedMilliseconds}ms.");
            }

            return finalIconPath;
        }

        private async Task<(string? Title, List<FaviconCandidate> Candidates)> FetchAndParseHtmlHeadAsync(Uri pageUri)
        {
            var candidates = new List<FaviconCandidate>();
            string? title = null;

            try
            {
                var response = await _httpClient.GetAsync(pageUri, HttpCompletionOption.ResponseHeadersRead).ConfigureAwait(false);
                if (!response.IsSuccessStatusCode) return (null, candidates);

                string? headContent;
                using (Stream stream = await response.Content.ReadAsStreamAsync().ConfigureAwait(false))
                {
                    headContent = await ReadHeadContentAsync(stream).ConfigureAwait(false);
                }

                if (string.IsNullOrEmpty(headContent)) return (null, candidates);

                var titleMatch = TitleRegex.Match(headContent);
                if (titleMatch.Success)
                {
                    title = System.Net.WebUtility.HtmlDecode(titleMatch.Groups[1].Value.Trim());
                }

                foreach (Match linkMatch in LinkTagRegex.Matches(headContent))
                {
                    var linkTag = linkMatch.Value;
                    var relMatch = RelAttributeRegex.Match(linkTag);
                    if (!relMatch.Success) continue;

                    var relValue = relMatch.Groups["v"].Value;
                    var relParts = relValue.Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries);
                    var isIcon = relParts.Any(p =>
                        p.Equals("icon", StringComparison.OrdinalIgnoreCase) ||
                        p.Equals("shortcut", StringComparison.OrdinalIgnoreCase) ||
                        p.Equals("apple-touch-icon", StringComparison.OrdinalIgnoreCase) ||
                        p.Equals("apple-touch-icon-precomposed", StringComparison.OrdinalIgnoreCase)
                    );
                    if (!isIcon) continue;

                    var hrefMatch = HrefAttributeRegex.Match(linkTag);
                    if (!hrefMatch.Success) continue;

                    var href = hrefMatch.Groups[1].Success ? hrefMatch.Groups[1].Value : hrefMatch.Groups[2].Value;
                    if (string.IsNullOrWhiteSpace(href)) continue;

                    var fullUrl = href.StartsWith("//", StringComparison.Ordinal)
                        ? $"{pageUri.Scheme}:{href}"
                        : new Uri(pageUri, href).ToString();

                    int score = CalculateFaviconScore(linkTag, fullUrl);
                    candidates.Add(new FaviconCandidate { Url = fullUrl, Score = score });
                }
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException)
            {
                LogManager.LogDebug($"Failed to fetch or parse HTML head for {pageUri}: {ex.Message}");
            }

            return (title, candidates);
        }

        private static int CalculateFaviconScore(string linkTag, string fullUrl)
        {
            int score = 0;
            string extension = Path.GetExtension(fullUrl).ToLowerInvariant();

            if (extension == ".svg") score = 10000;
            else if (extension == ".ico") score = 5000;
            else
            {
                var sizesMatch = SizesAttributeRegex.Match(linkTag);
                if (sizesMatch.Success)
                {
                    var sizesValue = sizesMatch.Groups["v"].Value.ToLowerInvariant();
                    if (sizesValue == "any") score = 100;
                    else
                    {
                        var firstSizePart = sizesValue.Split(' ')[0];
                        var dimension = firstSizePart.Split('x')[0];
                        if (int.TryParse(dimension, out int size))
                        {
                            score = size >= 32 ? 1000 - Math.Abs(size - 32) : size;
                        }
                    }
                }
                else
                {
                    score = 32;
                }
            }
            return score;
        }

        private static async Task<string?> ReadHeadContentAsync(Stream stream)
        {
            using var reader = new StreamReader(stream, Encoding.UTF8, true, 1024, leaveOpen: true);
            var contentBuilder = new StringBuilder();
            var buffer = new char[4096];
            int totalBytesRead = 0;
            const int maxKBToRead = 350;
            const int maxBytesToRead = maxKBToRead * 1024;
            var currentEncoding = reader.CurrentEncoding;

            while (totalBytesRead < maxBytesToRead)
            {
                int charsRead = await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false);
                if (charsRead == 0)
                {
                    break;
                }

                int bytesReadInThisChunk = currentEncoding.GetByteCount(buffer, 0, charsRead);
                totalBytesRead += bytesReadInThisChunk;

                contentBuilder.Append(buffer, 0, charsRead);

                string currentContent = contentBuilder.ToString();
                int headEndIndex = currentContent.IndexOf("</head>", StringComparison.OrdinalIgnoreCase);
                if (headEndIndex != -1)
                {
                    return currentContent.Substring(0, headEndIndex + "</head>".Length);
                }
            }

            if (totalBytesRead > 0)
            {
                LogManager.LogDebug($"HTML </head> tag not found within the first {maxKBToRead}KB. Using partial content for discovery.");
                return contentBuilder.ToString();
            }

            return null;
        }

        public async Task<string?> GetPageTitleAsync(Uri url)
        {
            ArgumentNullException.ThrowIfNull(url);
            var urlString = url.ToString();
            if (_titleCache.TryGetValue(urlString, out var cachedTitle))
            {
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
                var (title, _) = await FetchAndParseHtmlHeadAsync(url).ConfigureAwait(false);
                _titleCache.Add(urlString, title ?? string.Empty);
                return title;
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or IOException)
            {
                LogManager.Log(ex, $"Failed to fetch page title for {url}");
                _titleCache.Add(urlString, string.Empty);
                return null;
            }
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