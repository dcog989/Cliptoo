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
using Cliptoo.Core.Logging;
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
        private readonly ConcurrentDictionary<string, Task<string?>> _ongoingFetches = new();
        private bool _disposedValue;
        private readonly IImageDecoder _imageDecoder;

        public WebMetadataService(string appCachePath, IImageDecoder imageDecoder)
        {
            _faviconCacheDir = Path.Combine(appCachePath, "Cliptoo", "FaviconCache");
            Directory.CreateDirectory(_faviconCacheDir);

            _httpClient = new HttpClient();
            _httpClient.DefaultRequestHeaders.UserAgent.ParseAdd("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36");
            _httpClient.DefaultRequestHeaders.Accept.ParseAdd("text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8");
            _httpClient.DefaultRequestHeaders.AcceptLanguage.ParseAdd("en-US,en;q=0.9");
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

        public Task<string?> GetFaviconAsync(Uri url)
        {
            if (url is null) return Task.FromResult<string?>(null);

            var urlString = url.GetLeftPart(UriPartial.Authority);

            return _ongoingFetches.GetOrAdd(urlString, key => FetchAndCacheFaviconAsync(new Uri(key)));
        }

        private async Task<string?> FetchAndCacheFaviconAsync(Uri url)
        {
            var urlString = url.GetLeftPart(UriPartial.Authority);
            try
            {
                if (_failedFaviconUrls.ContainsKey(urlString)) return null;

                var successCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".png");
                if (File.Exists(successCachePath)) return successCachePath;

                var failureCachePath = ServiceUtils.GetCachePath(urlString, _faviconCacheDir, ".failed");
                if (File.Exists(failureCachePath))
                {
                    try
                    {
                        var timestampText = await File.ReadAllTextAsync(failureCachePath).ConfigureAwait(false);
                        if (DateTime.TryParse(timestampText, null, System.Globalization.DateTimeStyles.RoundtripKind, out var timestamp) && (DateTime.UtcNow - timestamp) < FailureCacheDuration)
                        {
                            _failedFaviconUrls.TryAdd(urlString, true);
                            LogManager.LogDebug($"FAVICON_CACHE_DIAG: Hit recent failure cache for '{urlString}'. Skipping fetch.");
                            return null;
                        }
                    }
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                    {
                        LogManager.LogDebug($"Failed to read failure cache file for {urlString}. Error: {ex.Message}");
                    }
                }

                LogManager.LogDebug($"FAVICON_CACHE_DIAG: Miss for '{urlString}'. Starting fetch process.");
                var stopwatch = System.Diagnostics.Stopwatch.StartNew();

                using var overallCts = new CancellationTokenSource(TimeSpan.FromSeconds(10));

                // Task 1: Fetch HTML and parse for candidates
                var htmlParseTask = FetchAndParseHtmlHeadAsync(url);

                // Task 2: Try fetching the common /favicon.ico in parallel
                var icoTask = FetchAndProcessFavicon(new Uri(url, "/favicon.ico").ToString(), successCachePath, overallCts.Token);

                await Task.WhenAll(htmlParseTask, icoTask).ConfigureAwait(false);

                // If favicon.ico was found, we are done
                if (await icoTask.ConfigureAwait(false))
                {
                    LogManager.LogDebug($"FAVICON_DISCOVERY_DIAG: Success with direct fetch of /favicon.ico for {urlString}");
                    return successCachePath;
                }

                var (pageTitle, htmlCandidates) = await htmlParseTask.ConfigureAwait(false);
                if (pageTitle != null) _titleCache.Add(url.ToString(), pageTitle);

                // If HTML parsing yielded candidates, try fetching them
                if (htmlCandidates.Count > 0)
                {
                    var orderedCandidates = htmlCandidates.OrderByDescending(c => c.Score).Select(c => c.Url).Distinct();
                    foreach (var candidateUrl in orderedCandidates)
                    {
                        if (await FetchAndProcessFavicon(candidateUrl, successCachePath, overallCts.Token).ConfigureAwait(false))
                        {
                            LogManager.LogDebug($"FAVICON_DISCOVERY_DIAG: Success with HTML-parsed candidate {candidateUrl} for {urlString}");
                            return successCachePath;
                        }
                    }
                }

                // Final fallbacks if all else fails
                if (await FetchAndProcessFavicon(new Uri(url, "/favicon.svg").ToString(), successCachePath, overallCts.Token).ConfigureAwait(false))
                {
                    LogManager.LogDebug($"FAVICON_DISCOVERY_DIAG: Success with fallback fetch of /favicon.svg for {urlString}");
                    return successCachePath;
                }
                if (await FetchAndProcessFavicon(new Uri(url, "/favicon.png").ToString(), successCachePath, overallCts.Token).ConfigureAwait(false))
                {
                    LogManager.LogDebug($"FAVICON_DISCOVERY_DIAG: Success with fallback fetch of /favicon.png for {urlString}");
                    return successCachePath;
                }

                // If we reach here, all attempts failed
                LogManager.LogDebug($"Favicon Discovery for {urlString}: All stages failed. Caching failure.");
                _failedFaviconUrls.TryAdd(urlString, true);
                try
                {
                    await File.WriteAllTextAsync(failureCachePath, DateTime.UtcNow.ToString("o")).ConfigureAwait(false);
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { LogManager.LogDebug($"Failed to create/update failure cache file for {urlString}. Error: {ex.Message}"); }

                stopwatch.Stop();
                LogManager.LogDebug($"PERF_DIAG: Favicon discovery for '{urlString}' took {stopwatch.ElapsedMilliseconds}ms.");
                return null;
            }
            finally
            {
                _ongoingFetches.TryRemove(urlString, out _);
            }
        }

        private async Task<(string? Title, List<FaviconCandidate> Candidates)> FetchAndParseHtmlHeadAsync(Uri pageUri)
        {
            var candidates = new List<FaviconCandidate>();
            string? title = null;

            try
            {
                var response = await _httpClient.GetAsync(pageUri, HttpCompletionOption.ResponseHeadersRead).ConfigureAwait(false);
                if (!response.IsSuccessStatusCode) return (null, candidates);

                var baseUri = response.RequestMessage?.RequestUri ?? pageUri;

                string? headContent;
                using (Stream stream = await response.Content.ReadAsStreamAsync().ConfigureAwait(false))
                {
                    headContent = await ReadHeadContentAsync(stream).ConfigureAwait(false);
                }

                if (string.IsNullOrEmpty(headContent)) return (null, candidates);

                var baseTagRegex = new Regex("<base[^>]+href\\s*=\\s*(?:['\"](?<v>[^'\"]*)['\"]|(?<v>[^>\\s]+))", RegexOptions.IgnoreCase | RegexOptions.Compiled);
                var baseMatch = baseTagRegex.Match(headContent);
                if (baseMatch.Success)
                {
                    var baseHref = baseMatch.Groups["v"].Value;
                    if (Uri.TryCreate(baseHref, UriKind.Absolute, out var parsedBaseUri))
                    {
                        baseUri = parsedBaseUri;
                    }
                    else if (Uri.TryCreate(baseUri, baseHref, out var combinedBaseUri))
                    {
                        baseUri = combinedBaseUri;
                    }
                }

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

                    if (relValue.IndexOf("icon", StringComparison.OrdinalIgnoreCase) < 0)
                    {
                        continue;
                    }

                    var hrefMatch = HrefAttributeRegex.Match(linkTag);
                    if (!hrefMatch.Success) continue;

                    var href = hrefMatch.Groups[1].Success ? hrefMatch.Groups[1].Value : hrefMatch.Groups[2].Value;
                    if (string.IsNullOrWhiteSpace(href)) continue;

                    var fullUrl = new Uri(baseUri, href).ToString();

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
            string extension = Path.GetExtension(fullUrl).ToUpperInvariant();

            if (extension == ".SVG") score = 10000;
            else if (extension == ".ICO") score = 5000;
            else
            {
                var sizesMatch = SizesAttributeRegex.Match(linkTag);
                if (sizesMatch.Success)
                {
                    var sizesValue = sizesMatch.Groups["v"].Value.ToUpperInvariant();
                    if (sizesValue == "ANY") score = 100;
                    else
                    {
                        var firstSizePart = sizesValue.Split(' ')[0];
                        var dimension = firstSizePart.Split('X')[0];
                        if (int.TryParse(dimension, out int size))
                        {
                            score = size >= 32 ? 1000 - Math.Abs(size - 32) : size;
                        }
                    }
                }
                else
                {
                    score = 32; // Default score for PNGs without size
                }
            }
            return score;
        }

        private static async Task<string?> ReadHeadContentAsync(Stream stream)
        {
            const int maxBytesToRead = 777 * 1024;
            using var memoryStream = new MemoryStream();
            Memory<byte> buffer = new byte[8192];
            long totalBytesRead = 0;

            while (totalBytesRead < maxBytesToRead)
            {
                var bytesToRead = (int)Math.Min(buffer.Length, maxBytesToRead - totalBytesRead);
                var bytesRead = await stream.ReadAsync(buffer.Slice(0, bytesToRead)).ConfigureAwait(false);
                if (bytesRead == 0)
                {
                    break;
                }
                await memoryStream.WriteAsync(buffer.Slice(0, bytesRead)).ConfigureAwait(false);
                totalBytesRead += bytesRead;
            }

            memoryStream.Position = 0;
            if (memoryStream.Length == 0) return null;

            using var reader = new StreamReader(memoryStream, Encoding.UTF8, true);
            var content = await reader.ReadToEndAsync().ConfigureAwait(false);

            if (string.IsNullOrEmpty(content))
            {
                return null;
            }

            const string headTag = "</head>";
            int headEndIndex = content.IndexOf(headTag, StringComparison.OrdinalIgnoreCase);
            if (headEndIndex != -1)
            {
                return content.Substring(0, headEndIndex + headTag.Length);
            }

            LogManager.LogDebug($"HTML </head> tag not found within the first {maxBytesToRead / 1024}KB. Using partial content for discovery.");
            return content;
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
                LogManager.LogDebug($"Failed to fetch page title for {url}: {ex.Message}");
                _titleCache.Add(urlString, string.Empty);
                return null;
            }
        }

        private async Task<bool> FetchAndProcessFavicon(string faviconUrl, string cachePath, CancellationToken cancellationToken = default)
        {
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            try
            {
                if (cancellationToken.IsCancellationRequested) return false;
                LogManager.LogDebug($"Attempting to fetch favicon: {faviconUrl}");

                if (faviconUrl.StartsWith("data:", StringComparison.OrdinalIgnoreCase))
                {
                    var result = await ProcessDataUriFaviconAsync(faviconUrl, cachePath).ConfigureAwait(false);
                    return !string.IsNullOrEmpty(result);
                }

                if (!Uri.TryCreate(faviconUrl, UriKind.Absolute, out var faviconUri) ||
                    (faviconUri.Scheme != Uri.UriSchemeHttp && faviconUri.Scheme != Uri.UriSchemeHttps))
                {
                    LogManager.LogDebug($"Invalid or unsupported URI scheme for favicon: {faviconUrl}");
                    return false;
                }

                var response = await _httpClient.GetAsync(faviconUri, cancellationToken).ConfigureAwait(false);
                if (!response.IsSuccessStatusCode)
                {
                    LogManager.LogDebug($"Favicon GET request for {faviconUrl} failed with status code {response.StatusCode}.");
                    return false;
                }

                byte[]? outputBytes = null;
                var extension = Path.GetExtension(faviconUri.AbsolutePath).ToUpperInvariant();

                if (extension == ".SVG")
                {
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
            catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException or ImageFormatException or NotSupportedException)
            {
                LogManager.LogDebug($"Favicon fetch/process failed for {faviconUrl}: {ex.Message}");
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
                LogManager.LogInfo("Web metadata caches cleared successfully.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                LogManager.LogError($"Failed to clear web caches. Error: {ex.Message}");
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
                LogManager.LogWarning($"Failed to clear cache for URL: {urlString}. Error: {ex.Message}");
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
                    if (fileInfo.CreationTimeUtc < cutoff)
                    {
                        filesToDelete.Add(file);
                    }
                }
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or DirectoryNotFoundException)
            {
                LogManager.LogError($"Failed to enumerate files for pruning in {_faviconCacheDir}. Error: {ex.Message}");
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