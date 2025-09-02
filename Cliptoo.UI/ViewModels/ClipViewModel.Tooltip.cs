using System.IO;
using System.Text;
using Cliptoo.Core;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Configuration;
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
            ClipType is AppConstants.ClipTypes.FileText or AppConstants.ClipTypes.Dev ||
            (ClipType == AppConstants.ClipTypes.Document &&
            (Content.EndsWith(".md", StringComparison.OrdinalIgnoreCase) ||
                Content.EndsWith(".markdown", StringComparison.OrdinalIgnoreCase) ||
                Content.EndsWith(".csv", StringComparison.OrdinalIgnoreCase)));

        public bool ShowFileInfoTooltip => IsFileBased && !IsImage && !IsPreviewableAsTextFile;
        public bool ShowTextualTooltip => IsPreviewableAsTextFile || (!IsFileBased && !IsLinkToolTip && ClipType != AppConstants.ClipTypes.Color);

        public async Task LoadTooltipContentAsync()
        {
            if (_isTooltipContentLoaded) return;
            DebugUtils.LogMemoryUsage($"LoadTooltipContentAsync START (ID: {Id})");

            var clipForTooltip = await GetFullClipAsync().ConfigureAwait(false);

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
                catch (Exception ex)
                {
                    LogManager.Log(ex, "Failed to read text file for tooltip preview.");
                    if (ex is IOException && (uint)ex.HResult == 0x80070020)
                    {
                        textFileContent = "Error: File is in use.";
                    }
                    else
                    {
                        textFileContent = $"Error reading file: {ex.Message}";
                    }
                }
            }

            var loadTasks = new List<Task>();
            if (IsFileBased)
            {
                loadTasks.Add(LoadFilePropertiesAsync());
            }
            if (IsLinkToolTip)
            {
                loadTasks.Add(LoadPageTitleAsync());
            }

            await Task.WhenAll(loadTasks);

            GenerateTooltipProperties(clipForTooltip, textFileContent);
            _isTooltipContentLoaded = true;
            DebugUtils.LogMemoryUsage($"LoadTooltipContentAsync END (ID: {Id})");
        }

        public void ClearTooltipContent()
        {
            TooltipTextContent = null;
            LineCountInfo = null;
            _isTooltipContentLoaded = false;
            OnPropertyChanged(nameof(TooltipTextContent));
            OnPropertyChanged(nameof(LineCountInfo));
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
                            // Line number padding will be calculated later
                            sb.AppendLine(line);
                        }
                    }
                }

                if (totalLines == 0 && contentForTooltip.Length > 0 && !contentForTooltip.Contains('\n'))
                {
                    totalLines = 1;
                }

                // Now format the output with correct padding
                var finalSb = new StringBuilder();
                var lines = sb.ToString().Split(new[] { Environment.NewLine }, StringSplitOptions.None);
                int numberPadding = totalLines.ToString().Length;

                for (int i = 0; i < linesProcessed; i++)
                {
                    finalSb.AppendLine($"{(i + 1).ToString().PadLeft(numberPadding)} | {lines[i]}");
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

                if (totalLines > MaxTooltipLines)
                {
                    finalSb.AppendLine($"\n... (truncated - {totalLines - MaxTooltipLines} more lines)");
                }

                TooltipTextContent = finalSb.ToString();
            }
            else
            {
                TooltipTextContent = null;
                LineCountInfo = null;
            }

            OnPropertyChanged(nameof(TooltipTextContent));
            OnPropertyChanged(nameof(LineCountInfo));
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

            var imagePreviewPath = await _clipDetailsLoader.GetImagePreviewAsync(this, _thumbnailService, largestDimension, _theme);

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
                catch (Exception ex)
                {
                    LogManager.Log(ex, $"Failed to load image preview from path: {imagePreviewPath}");
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
            _pageTitleCts?.Cancel();
            _pageTitleCts = new CancellationTokenSource();
            var token = _pageTitleCts.Token;

            if (IsPageTitleLoading || !string.IsNullOrEmpty(PageTitle)) return;
            IsPageTitleLoading = true;

            PageTitle = await _clipDetailsLoader.GetPageTitleAsync(this, _webMetadataService, token);

            if (!token.IsCancellationRequested)
            {
                IsPageTitleLoading = false;
            }
        }

        public async Task LoadFilePropertiesAsync()
        {
            _filePropertiesCts?.Cancel();
            _filePropertiesCts = new CancellationTokenSource();
            var token = _filePropertiesCts.Token;

            if (IsFilePropertiesLoading || !string.IsNullOrEmpty(FileProperties)) return;

            IsFilePropertiesLoading = true;
            FileProperties = null;
            FileTypeInfo = null;
            FileTypeInfoIcon = null;

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
        }
    }
}