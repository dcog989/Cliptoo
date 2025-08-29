using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Text;
using System.Text.RegularExpressions;
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

        private record FaviconCandidate(string Url, int Priority);

        private static readonly Regex LinkTagRegex = new("<link[^>]+>", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex RelAttributeRegex = new("rel\\s*=\\s*['\"][^'\"]*\\bicon\\b[^'\"]*['\"]", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex HrefAttributeRegex = new("href\\s*=\\s*(?:['\"]([^'\"]+)['\"]|([^>\\s]+))", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex SizesAttributeRegex = new("sizes\\s*=\\s*['\"](\\d+x\\d+)['\"]", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private static readonly Regex TitleRegex = new("<title[^>]*>\\s*(.+?)\\s*</title>", RegexOptions.IgnoreCase | RegexOptions.Compiled);
        private readonly LruCache<string, string> _titleCache;
        private readonly ConcurrentDictionary<string, bool> _failedFaviconUrls = new();
        private bool _disposedValue;

        public WebMetadataService(string appCachePath)
        {
            _faviconCacheDir = Path.Combine(appCachePath, "Cliptoo", "FaviconCache");
            Directory.CreateDirectory(_faviconCacheDir);

            _httpClient = new HttpClient();
            _httpClient.DefaultRequestHeaders.UserAgent.ParseAdd("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36");
            _httpClient.Timeout = TimeSpan.FromSeconds(5);

            _pngEncoder = new PngEncoder { CompressionLevel = PngCompressionLevel.Level6 };
            _titleCache = new LruCache<string, string>(100);
        }

        public async Task<string?> GetFaviconAsync(Uri url)
        {
            if (url is null) return null;
            var urlString = url.ToString();
            if (_failedFaviconUrls.ContainsKey(urlString)) return null;

            var successCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".png");
            if (File.Exists(successCachePath)) return successCachePath;

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

            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            string? finalIconPath = null;
            try
            {
                LogManager.LogDebug($"Favicon Discovery for {urlString}: Starting Stage 1 (Root Icon Check).");
                finalIconPath = await TryFetchRootIconsAsync(url, successCachePath).ConfigureAwait(false);

                if (finalIconPath == null)
                {
                    LogManager.LogDebug($"Favicon Discovery for {urlString}: Stage 1 failed. Starting Stage 2 (HTML Head Parse).");
                    finalIconPath = await TryFetchIconsFromHtmlAsync(url, successCachePath).ConfigureAwait(false);
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

        private async Task<string?> TryFetchRootIconsAsync(Uri baseUri, string cachePath)
        {
            var rootIconNames = new[] { "/favicon.ico", "/favicon.png", "/favicon.svg", "/favicon.webp" };
            foreach (var iconName in rootIconNames)
            {
                var faviconUrl = new Uri(baseUri, iconName);
                if (await FetchAndProcessFavicon(faviconUrl.ToString(), cachePath).ConfigureAwait(false))
                {
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
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException)
            {
                LogManager.Log(ex, $"Failed to fetch HTML head for {pageUri}");
                return null;
            }


            if (string.IsNullOrEmpty(headContent)) return null;

            var candidates = new List<FaviconCandidate>();
            foreach (Match linkMatch in LinkTagRegex.Matches(headContent))
            {
                var linkTag = linkMatch.Value;
                if (!RelAttributeRegex.IsMatch(linkTag)) continue;

                var hrefMatch = HrefAttributeRegex.Match(linkTag);
                if (!hrefMatch.Success) continue;

                var href = hrefMatch.Groups[1].Success ? hrefMatch.Groups[1].Value : hrefMatch.Groups[2].Value;
                var pathOnly = href.Split('?').First();
                var extension = Path.GetExtension(pathOnly).ToUpperInvariant();

                if (extension != ".PNG" && extension != ".ICO" && extension != ".WEBP" && extension != ".SVG")
                {
                    continue;
                }

                var sizesMatch = SizesAttributeRegex.Match(linkTag);
                var size = sizesMatch.Success ? sizesMatch.Groups[1].Value : string.Empty;
                var priority = GetPriorityFromSize(size);

                string fullUrl;
                if (href.StartsWith("//", StringComparison.Ordinal))
                {
                    fullUrl = $"{pageUri.Scheme}:{href}";
                }
                else
                {
                    fullUrl = new Uri(pageUri, href).ToString();
                }

                candidates.Add(new FaviconCandidate(fullUrl, priority));
            }

            foreach (var candidate in candidates.OrderBy(c => c.Priority))
            {
                if (await FetchAndProcessFavicon(candidate.Url, cachePath).ConfigureAwait(false))
                {
                    return cachePath;
                }
            }

            return null;
        }

        private static async Task<string?> ReadHeadContentAsync(Stream stream)
        {
            using var reader = new StreamReader(stream, Encoding.UTF8, true, 1024, true);
            var buffer = new char[4096];
            var content = new StringBuilder();
            bool inScript = false;
            bool inStyle = false;

            while (true)
            {
                int charsRead = await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false);
                if (charsRead == 0) break;

                content.Append(buffer, 0, charsRead);
                var currentContent = content.ToString();

                int searchIndex = 0;
                while (searchIndex < currentContent.Length)
                {
                    int scriptIndex = currentContent.IndexOf(inScript ? "</script>" : "<script", searchIndex, StringComparison.OrdinalIgnoreCase);
                    int styleIndex = currentContent.IndexOf(inStyle ? "</style>" : "<style", searchIndex, StringComparison.OrdinalIgnoreCase);
                    int headIndex = currentContent.IndexOf("</head>", searchIndex, StringComparison.OrdinalIgnoreCase);

                    int firstIndex = new[] { scriptIndex, styleIndex, headIndex }.Where(i => i != -1).DefaultIfEmpty(-1).Min();

                    if (firstIndex == -1)
                    {
                        searchIndex = currentContent.Length;
                        continue;
                    }

                    if (firstIndex == headIndex && !inScript && !inStyle)
                    {
                        return currentContent.Substring(0, headIndex + "</head>".Length);
                    }
                    if (firstIndex == scriptIndex)
                    {
                        inScript = !inScript;
                        searchIndex = scriptIndex + (inScript ? "<script".Length : "</script>".Length);
                    }
                    else if (firstIndex == styleIndex)
                    {
                        inStyle = !inStyle;
                        searchIndex = styleIndex + (inStyle ? "<style".Length : "</style>".Length);
                    }
                }

                if (content.Length > 100 * 1024)
                {
                    LogManager.LogDebug("HTML head section not found within the first 100KB. Aborting parse.");
                    return content.ToString();
                }
            }

            return content.ToString();
        }


        private static int GetPriorityFromSize(string size)
        {
            return size switch
            {
                "32x32" => 0,
                "48x48" => 1,
                "24x24" => 2,
                var s when !string.IsNullOrEmpty(s) => 3,
                _ => 4
            };
        }

        public async Task<string?> GetPageTitleAsync(Uri url)
        {
            ArgumentNullException.ThrowIfNull(url);
            var urlString = url.ToString();
            if (_titleCache.TryGetValue(urlString, out var cachedTitle))
            {
                return string.IsNullOrEmpty(cachedTitle) ? null : cachedTitle;
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

        private async Task<bool> FetchAndProcessFavicon(string faviconUrl, string cachePath)
        {
            try
            {
                LogManager.LogDebug($"Attempting to fetch favicon: {faviconUrl}");
                if (!Uri.TryCreate(faviconUrl, UriKind.Absolute, out var faviconUri)) return false;
                var response = await _httpClient.GetAsync(faviconUri).ConfigureAwait(false);

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
                    var svgContent = await response.Content.ReadAsStringAsync().ConfigureAwait(false);
                    outputBytes = await ServiceUtils.GenerateSvgPreviewAsync(svgContent, ThumbnailSize, "light", true).ConfigureAwait(false);
                }
                else
                {
                    var contentStream = await response.Content.ReadAsStreamAsync().ConfigureAwait(false);
                    await using (contentStream.ConfigureAwait(false))
                    {
                        using var image = await ImageDecoder.DecodeAsync(contentStream, extension).ConfigureAwait(false);
                        if (image != null)
                        {
                            image.Mutate(x => x.Resize(ThumbnailSize, ThumbnailSize));
                            using var ms = new MemoryStream();
                            await image.SaveAsPngAsync(ms, _pngEncoder).ConfigureAwait(false);
                            outputBytes = ms.ToArray();
                        }
                    }
                }

                if (outputBytes != null)
                {
                    await File.WriteAllBytesAsync(cachePath, outputBytes).ConfigureAwait(false);
                    LogManager.LogDebug($"Successfully processed and cached favicon for {faviconUrl}.");
                    return true;
                }
            }
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or ImageFormatException or NotSupportedException)
            {
                LogManager.Log(ex, $"Favicon fetch/process failed for {faviconUrl}.");
            }
            return false;
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
            var urlString = url.ToString();

            try
            {
                var successCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".png");
                if (File.Exists(successCachePath))
                {
                    File.Delete(successCachePath);
                }

                var failureCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".failed");
                if (File.Exists(failureCachePath))
                {
                    File.Delete(failureCachePath);
                }

                _failedFaviconUrls.TryRemove(urlString, out _);

                LogManager.LogDebug($"Cleared favicon cache for URL: {urlString}");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.Log(ex, $"Failed to clear cache for URL: {urlString}");
            }
        }

        public async Task<int> PruneCacheAsync(IAsyncEnumerable<string> validUrls)
        {
            ArgumentNullException.ThrowIfNull(validUrls);
            var validCacheFiles = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var enumerator = validUrls.GetAsyncEnumerator();
            try
            {
                while (await enumerator.MoveNextAsync().ConfigureAwait(false))
                {
                    var url = enumerator.Current;
                    validCacheFiles.Add(ServiceUtils.GetCachePath(url, _faviconCacheDir, ".png"));
                    validCacheFiles.Add(ServiceUtils.GetCachePath(url, _faviconCacheDir, ".failed"));
                }
            }
            finally
            {
                await enumerator.DisposeAsync().ConfigureAwait(false);
            }

            return await ServiceUtils.PruneDirectoryAsync(_faviconCacheDir, validCacheFiles).ConfigureAwait(false);
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