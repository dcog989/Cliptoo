using System.Globalization;
using System.IO;
using System.Text;
using Cliptoo.Core;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Logging;
using Cliptoo.UI.Helpers;
using System.Windows.Media.Imaging;

namespace Cliptoo.UI.ViewModels
{
    public partial class ClipViewModel
    {
        private const int MaxTooltipLines = 40;

        private bool _isTooltipContentLoaded;
        public bool IsImage => ClipType == AppConstants.ClipTypes.Image;
        public bool IsRtf => ClipType == AppConstants.ClipTypes.Rtf;
        public bool IsLinkToolTip => ClipType == AppConstants.ClipTypes.Link;
        public bool IsPreviewableAsTextFile =>
            (ClipType is AppConstants.ClipTypes.FileText or AppConstants.ClipTypes.Dev ||
            (ClipType == AppConstants.ClipTypes.Document &&
            (Content.EndsWith(".md", StringComparison.OrdinalIgnoreCase) ||
                Content.EndsWith(".markdown", StringComparison.OrdinalIgnoreCase) ||
                Content.EndsWith(".csv", StringComparison.OrdinalIgnoreCase))))
            && !string.Equals(Content, LogManager.LogFilePath, StringComparison.OrdinalIgnoreCase);

        public bool ShowFileInfoTooltip => IsFileBased && !IsImage && !IsPreviewableAsTextFile;
        public bool ShowTextualTooltip => IsPreviewableAsTextFile || (!IsFileBased && !IsLinkToolTip && ClipType != AppConstants.ClipTypes.Color);

        public async Task LoadTooltipContentAsync()
        {
            if (_isTooltipContentLoaded) return;
            DebugUtils.LogMemoryUsage($"LoadTooltipContentAsync START (ID: {Id})");
            var clipForTooltip = await GetFullClipAsync();

            if (clipForTooltip is null)
            {
                return;
            }

            string? textFileContent = null;
            if (IsPreviewableAsTextFile)
            {
                try
                {
                    if (File.Exists(Content))
                    {
                        using var reader = new StreamReader(Content, true);
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
            DebugUtils.LogMemoryUsage($"LoadTooltipContentAsync END (ID: {Id})");
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
            if (clipToDisplay.ClipType == AppConstants.ClipTypes.Image)
            {
                TooltipTextContent = null;
                LineCountInfo = null;
                return;
            }

            string contentForTooltip = textFileContent ?? (clipToDisplay.ClipType == AppConstants.ClipTypes.Rtf
                ? RtfUtils.ToPlainText(clipToDisplay.Content ?? "")
                : clipToDisplay.Content ?? "");

            bool wasTruncatedByCharLimit = false;
            const int MaxTooltipChars = 16 * 1024;
            if (contentForTooltip.Length > MaxTooltipChars)
            {
                contentForTooltip = contentForTooltip.Substring(0, MaxTooltipChars);
                wasTruncatedByCharLimit = true;
            }

            if (ShowTextualTooltip && !string.IsNullOrEmpty(contentForTooltip))
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

                if (!IsFileBased)
                {
                    var formattedSize = FormatUtils.FormatBytes(SizeInBytes);
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
            var isMissing = !File.Exists(Content);
            if (isMissing != IsSourceMissing)
            {
                IsSourceMissing = isMissing;
            }

            if (isMissing)
            {
                ImagePreviewSource = null;
                return;
            }

            var imagePreviewPath = await _clipDetailsLoader.GetImagePreviewAsync(this, _thumbnailService, largestDimension, _theme).ConfigureAwait(false);

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

            PageTitle = await _clipDetailsLoader.GetPageTitleAsync(this, _webMetadataService, token).ConfigureAwait(false);

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
            var (properties, typeInfo, isMissing) = await _clipDetailsLoader.GetFilePropertiesAsync(this, token);

            if (!token.IsCancellationRequested)
            {
                FileProperties = properties;
                FileTypeInfo = typeInfo;
                IsSourceMissing = isMissing;
                IsFilePropertiesLoading = false;

                if (!isMissing && !string.IsNullOrEmpty(ClipType))
                {
                    FileTypeInfoIcon = await _iconProvider.GetIconAsync(this.ClipType, 16);
                }
            }
#pragma warning restore CA2007
        }

    }
}