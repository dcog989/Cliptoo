using System.Globalization;
using System.IO;
using System.Text;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cliptoo.Core;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;

namespace Cliptoo.UI.ViewModels
{
    internal class ClipViewModelDetails : ViewModelBase, IDisposable
    {
        private const int MaxTooltipLines = 40;

        private readonly ClipViewModel _owner;
        private readonly IClipDataService _clipDataService;
        private readonly IClipDetailsLoader _clipDetailsLoader;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;
        private readonly IIconProvider _iconProvider;
        private string _theme;
        private bool _isTooltipContentLoaded;
        private int _currentThumbnailLoadId;

        private ImageSource? _thumbnailSource;
        public ImageSource? ThumbnailSource { get => _thumbnailSource; private set => SetProperty(ref _thumbnailSource, value); }
        private bool _isThumbnailLoading;
        public bool IsThumbnailLoading { get => _isThumbnailLoading; private set => SetProperty(ref _isThumbnailLoading, value); }
        private bool _hasThumbnail;
        public bool HasThumbnail { get => _hasThumbnail; private set => SetProperty(ref _hasThumbnail, value); }
        private ImageSource? _clipTypeIcon;
        public ImageSource? ClipTypeIcon { get => _clipTypeIcon; private set => SetProperty(ref _clipTypeIcon, value); }
        private ImageSource? _quickPasteIcon;
        public ImageSource? QuickPasteIcon { get => _quickPasteIcon; private set => SetProperty(ref _quickPasteIcon, value); }
        private ImageSource? _imagePreviewSource;
        public ImageSource? ImagePreviewSource { get => _imagePreviewSource; private set => SetProperty(ref _imagePreviewSource, value); }
        private string? _fileProperties;
        public string? FileProperties { get => _fileProperties; private set => SetProperty(ref _fileProperties, value); }
        private string? _fileTypeInfo;
        public string? FileTypeInfo { get => _fileTypeInfo; private set => SetProperty(ref _fileTypeInfo, value); }
        private bool _isFilePropertiesLoading;
        public bool IsFilePropertiesLoading { get => _isFilePropertiesLoading; private set => SetProperty(ref _isFilePropertiesLoading, value); }
        private bool _isSourceMissing;
        public bool IsSourceMissing
        {
            get => _isSourceMissing;
            private set => SetProperty(ref _isSourceMissing, value);
        }
        private ImageSource? _fileTypeInfoIcon;
        public ImageSource? FileTypeInfoIcon { get => _fileTypeInfoIcon; private set => SetProperty(ref _fileTypeInfoIcon, value); }
        private string? _pageTitle;
        public string? PageTitle { get => _pageTitle; private set => SetProperty(ref _pageTitle, value); }
        private bool _isPageTitleLoading;
        public bool IsPageTitleLoading { get => _isPageTitleLoading; private set => SetProperty(ref _isPageTitleLoading, value); }
        private string? _tooltipTextContent;
        public string? TooltipTextContent { get => _tooltipTextContent; private set => SetProperty(ref _tooltipTextContent, value); }
        private string? _lineCountInfo;
        public string? LineCountInfo { get => _lineCountInfo; private set => SetProperty(ref _lineCountInfo, value); }

        private CancellationTokenSource? _filePropertiesCts;
        private CancellationTokenSource? _pageTitleCts;

        public ClipViewModelDetails(ClipViewModel owner, IClipDataService clipDataService, IClipDetailsLoader clipDetailsLoader, IThumbnailService thumbnailService, IWebMetadataService webMetadataService, IIconProvider iconProvider)
        {
            _owner = owner;
            _clipDataService = clipDataService;
            _clipDetailsLoader = clipDetailsLoader;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
            _iconProvider = iconProvider;
            _theme = "light";
        }

        public void UpdateTheme(string theme) => _theme = theme;

        public async Task UpdateSourceMissingStateAsync()
        {
            bool isMissing = false;
            if (_owner.IsFileBased && !string.IsNullOrEmpty(_owner.Content))
            {
                isMissing = await Task.Run(() =>
                {
                    var path = _owner.Content.Trim();
                    if (_owner.ClipType == AppConstants.ClipTypeFolder)
                    {
                        return !Directory.Exists(path);
                    }
                    return !File.Exists(path);
                }).ConfigureAwait(false);
            }
            IsSourceMissing = isMissing;
        }

        public async Task LoadThumbnailAsync()
        {
            if (IsThumbnailLoading) return;

            IsThumbnailLoading = true;
            try
            {
                var loadId = Interlocked.Increment(ref _currentThumbnailLoadId);
                string? newThumbnailPath = await _clipDetailsLoader.GetThumbnailAsync(_owner, _thumbnailService, _webMetadataService, _theme).ConfigureAwait(false);

                if (loadId != _currentThumbnailLoadId)
                {
                    return;
                }

                BitmapImage? finalBitmap = null;
                if (!string.IsNullOrEmpty(newThumbnailPath))
                {
                    try
                    {
                        byte[]? bytes = null;
                        for (int i = 0; i < 3; i++)
                        {
                            try
                            {
                                bytes = await File.ReadAllBytesAsync(newThumbnailPath).ConfigureAwait(false);
                                break;
                            }
                            catch (IOException)
                            {
                                await Task.Delay(50).ConfigureAwait(false);
                            }
                        }
                        if (bytes != null)
                        {
                            using var ms = new MemoryStream(bytes);
                            var bitmapImage = new BitmapImage();
                            bitmapImage.BeginInit();
                            bitmapImage.StreamSource = ms;
                            bitmapImage.CacheOption = BitmapCacheOption.OnLoad;
                            bitmapImage.EndInit();
                            bitmapImage.Freeze();
                            finalBitmap = bitmapImage;
                        }
                    }
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or NotSupportedException)
                    {
                        LogManager.LogWarning($"Failed to load thumbnail image source from path: {newThumbnailPath}. Error: {ex.Message}");
                    }
                }

                await System.Windows.Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    ThumbnailSource = finalBitmap;
                    HasThumbnail = finalBitmap != null;
                });
            }
            finally
            {
                IsThumbnailLoading = false;
            }
        }

        public async Task LoadTooltipContentAsync()
        {
            if (_isTooltipContentLoaded) return;
            DebugUtils.LogMemoryUsage($"LoadTooltipContentAsync START (ID: {_owner.Id})");
            var clipForTooltip = await _owner.GetFullClipAsync();

            if (clipForTooltip is null)
            {
                return;
            }

            string? textFileContent = null;
            if (_owner.IsPreviewableAsTextFile)
            {
                try
                {
                    if (File.Exists(_owner.Content))
                    {
                        using var reader = new StreamReader(_owner.Content, true);
                        var buffer = new char[4096];
                        int charsRead = await reader.ReadAsync(buffer, 0, buffer.Length);
                        textFileContent = new string(buffer, 0, charsRead);
                    }
                }
                catch (IOException ex)
                {
                    LogManager.LogWarning($"Failed to read text file for tooltip preview. Error: {ex.Message}");
                    if ((uint)ex.HResult == 0x80070020)
                    {
                        textFileContent = "Error: File is in use.";
                    }
                    else
                    {
                        textFileContent = $"Error reading file: {ex.Message}";
                    }
                }
            }

            var loadTasks = new List<Task>
            {
                LoadFilePropertiesAsync(),
                LoadPageTitleAsync()
            };

            await Task.WhenAll(loadTasks);

            GenerateTooltipProperties(clipForTooltip, textFileContent);
            _isTooltipContentLoaded = true;
            DebugUtils.LogMemoryUsage($"LoadTooltipContentAsync END (ID: {_owner.Id})");
        }

        public void ClearTooltipContent()
        {
            if (!_isTooltipContentLoaded) return;

#pragma warning disable CA1849 // Use CancelAsync when available
            _pageTitleCts?.Cancel();
            _filePropertiesCts?.Cancel();
#pragma warning restore CA1849

            TooltipTextContent = null;
            LineCountInfo = null;
            ImagePreviewSource = null;
            PageTitle = null;
            FileProperties = null;
            FileTypeInfo = null;
            FileTypeInfoIcon = null;

            _isTooltipContentLoaded = false;
            IsPageTitleLoading = false;
            IsFilePropertiesLoading = false;
        }

        private void GenerateTooltipProperties(Clip clipToDisplay, string? textFileContent = null)
        {
            if (clipToDisplay.ClipType == AppConstants.ClipTypeImage)
            {
                TooltipTextContent = null;
                LineCountInfo = null;
                return;
            }

            string contentForTooltip = textFileContent ?? (clipToDisplay.ClipType == AppConstants.ClipTypeRtf
                ? RtfUtils.ToPlainText(clipToDisplay.Content ?? "")
                : clipToDisplay.Content ?? "");

            bool wasTruncatedByCharLimit = false;
            const int MaxTooltipChars = 16 * 1024;
            if (contentForTooltip.Length > MaxTooltipChars)
            {
                contentForTooltip = contentForTooltip.Substring(0, MaxTooltipChars);
                wasTruncatedByCharLimit = true;
            }

            if (_owner.ShowTextualTooltip && !string.IsNullOrEmpty(contentForTooltip))
            {
                var sb = new StringBuilder();
                int totalLines = 0;
                int linesProcessed = 0;

                using (var reader = new StringReader(contentForTooltip))
                {
                    string? line;
                    while ((line = reader.ReadLine()) != null)
                    {
                        totalLines++;
                        if (linesProcessed < MaxTooltipLines)
                        {
                            linesProcessed++;
                            sb.AppendLine(line);
                        }
                    }
                }

                if (totalLines == 0 && contentForTooltip.Length > 0 && !contentForTooltip.Contains('\n', StringComparison.Ordinal))
                {
                    totalLines = 1;
                }

                var finalSb = new StringBuilder();
                var lines = sb.ToString().Split(new[] { Environment.NewLine }, StringSplitOptions.None);
                int numberPadding = totalLines.ToString(CultureInfo.InvariantCulture).Length;

                for (int i = 0; i < linesProcessed; i++)
                {
                    finalSb.AppendLine(CultureInfo.InvariantCulture, $"{(i + 1).ToString(CultureInfo.InvariantCulture).PadLeft(numberPadding)} | {lines[i]}");
                }

                if (!_owner.IsFileBased)
                {
                    var formattedSize = FormatUtils.FormatBytes(_owner.SizeInBytes);
                    var lineInfo = totalLines > 1 ? $", {totalLines} lines" : "";
                    LineCountInfo = $"Size: {formattedSize}{lineInfo}";
                }
                else
                {
                    LineCountInfo = null;
                }

                if (totalLines > MaxTooltipLines || wasTruncatedByCharLimit)
                {
                    var reason = totalLines > MaxTooltipLines
                        ? $"{totalLines - MaxTooltipLines} more lines"
                        : "content too large";
                    finalSb.AppendLine(CultureInfo.InvariantCulture, $"\n... (truncated - {reason})");
                }

                TooltipTextContent = finalSb.ToString();
            }
            else
            {
                TooltipTextContent = null;
                LineCountInfo = null;
            }
        }

        public async Task LoadImagePreviewAsync(uint largestDimension)
        {
            var isMissing = !File.Exists(_owner.Content);
            if (isMissing != IsSourceMissing)
            {
                IsSourceMissing = isMissing;
            }

            if (isMissing)
            {
                ImagePreviewSource = null;
                return;
            }

            var imagePreviewPath = await _clipDetailsLoader.GetImagePreviewAsync(_owner, _thumbnailService, largestDimension, _theme).ConfigureAwait(false);

            if (!string.IsNullOrEmpty(imagePreviewPath))
            {
                try
                {
                    var bytes = await File.ReadAllBytesAsync(imagePreviewPath).ConfigureAwait(false);
                    using var ms = new MemoryStream(bytes);

                    var bitmapImage = new BitmapImage();
                    bitmapImage.BeginInit();
                    bitmapImage.StreamSource = ms;
                    bitmapImage.CacheOption = BitmapCacheOption.OnLoad;
                    bitmapImage.EndInit();
                    bitmapImage.Freeze();
                    ImagePreviewSource = bitmapImage;
                }
                catch (IOException ex)
                {
                    LogManager.LogWarning($"Failed to load image preview from path: {imagePreviewPath}. Error: {ex.Message}");
                    ImagePreviewSource = null;
                }
            }
            else
            {
                ImagePreviewSource = null;
            }
        }

        public async Task LoadPageTitleAsync()
        {
            if (_pageTitleCts is not null) await _pageTitleCts.CancelAsync().ConfigureAwait(false);
            _pageTitleCts = new CancellationTokenSource();
            var token = _pageTitleCts.Token;

            if (IsPageTitleLoading || !string.IsNullOrEmpty(PageTitle)) return;
            IsPageTitleLoading = true;

            PageTitle = await _clipDetailsLoader.GetPageTitleAsync(_owner, _webMetadataService, token).ConfigureAwait(false);

            if (!token.IsCancellationRequested)
            {
                IsPageTitleLoading = false;
            }
        }

        public async Task LoadFilePropertiesAsync()
        {
            if (_filePropertiesCts is not null) await _filePropertiesCts.CancelAsync().ConfigureAwait(false);
            _filePropertiesCts = new CancellationTokenSource();
            var token = _filePropertiesCts.Token;

            if (IsFilePropertiesLoading || !string.IsNullOrEmpty(FileProperties)) return;

            IsFilePropertiesLoading = true;
            FileProperties = null;
            FileTypeInfo = null;
            FileTypeInfoIcon = null;

#pragma warning disable CA2007
            var (properties, typeInfo, isMissing) = await _clipDetailsLoader.GetFilePropertiesAsync(_owner, token);

            if (!token.IsCancellationRequested)
            {
                FileProperties = properties;
                FileTypeInfo = typeInfo;
                IsSourceMissing = isMissing;
                IsFilePropertiesLoading = false;

                if (!isMissing && !string.IsNullOrEmpty(_owner.ClipType))
                {
                    FileTypeInfoIcon = await _iconProvider.GetIconAsync(_owner.ClipType, 16);
                }
            }
#pragma warning restore CA2007
        }

        public async Task LoadIconsAsync()
        {
            ClipTypeIcon = await _iconProvider.GetIconAsync(_owner.ClipType, 20).ConfigureAwait(true);
        }

        public async Task LoadQuickPasteIconAsync()
        {
            if (_owner.Index > 0 && _owner.Index <= 9)
            {
                QuickPasteIcon = await _iconProvider.GetIconAsync(_owner.Index.ToString(CultureInfo.InvariantCulture), 32).ConfigureAwait(true);
            }
            else
            {
                QuickPasteIcon = null;
            }
        }

        public void ReleaseThumbnail()
        {
            Interlocked.Increment(ref _currentThumbnailLoadId);
            ThumbnailSource = null;
            HasThumbnail = false;
            IsThumbnailLoading = false;
        }

        public void Dispose()
        {
            _filePropertiesCts?.Dispose();
            _pageTitleCts?.Dispose();
        }
    }
}