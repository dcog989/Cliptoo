using System.Globalization;
using System.IO;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;

namespace Cliptoo.UI.ViewModels
{

    public partial class ClipViewModel : ViewModelBase, IDisposable
    {
        internal Clip _clip;
        private readonly IClipDetailsLoader _clipDetailsLoader;
        private readonly IIconProvider _iconProvider;
        private readonly IClipDataService _clipDataService;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;
        private ImageSource? _thumbnailSource;
        private bool _isThumbnailLoading;
        private bool _isPinned;
        private FontFamily _currentFontFamily = new("Segoe UI");
        private double _currentFontSize = 14;
        private string _paddingSize;
        private int _index;
        private ImageSource? _imagePreviewSource;
        private int _currentThumbnailLoadId;
        private bool _hasThumbnail;
        private string _theme = "light";
        private string? _fileProperties;
        private string? _fileTypeInfo;
        private bool _isFilePropertiesLoading;
        private bool _isSourceMissing;
        public bool IsSourceMissing
        {
            get => _isSourceMissing;
            private set
            {
                if (SetProperty(ref _isSourceMissing, value))
                {
                    OnPropertyChanged(nameof(IsOpenable));
                    OnPropertyChanged(nameof(ShowCompareMenu));
                }
            }
        }
        private CancellationTokenSource? _filePropertiesCts;
        private string? _pageTitle;
        private bool _isPageTitleLoading;
        private CancellationTokenSource? _pageTitleCts;
        private FontFamily _previewFont = new("Segoe UI");
        private double _previewFontSize = 14;
        private string _compareLeftHeader = "Compare Left";
        private bool _showCompareRightOption;
        private ImageSource? _clipTypeIcon;
        private ImageSource? _quickPasteIcon;
        private ImageSource? _fileTypeInfoIcon;
        private bool _disposedValue;
        private static readonly char[] _spaceSeparator = [' '];

        public ImageSource? ClipTypeIcon { get => _clipTypeIcon; private set => SetProperty(ref _clipTypeIcon, value); }
        public ImageSource? QuickPasteIcon { get => _quickPasteIcon; private set => SetProperty(ref _quickPasteIcon, value); }
        public ImageSource? FileTypeInfoIcon { get => _fileTypeInfoIcon; private set => SetProperty(ref _fileTypeInfoIcon, value); }
        public bool IsTextTransformable => IsEditable;
        public bool IsCompareToolAvailable => MainViewModel.IsCompareToolAvailable;
        public bool ShowCompareMenu => !IsSourceMissing && IsComparable && IsCompareToolAvailable;
        public MainViewModel MainViewModel { get; }

        public int Id => _clip.Id;
        public string Content => _clip.PreviewContent ?? string.Empty;
        public DateTime Timestamp => _clip.Timestamp;
        public string ClipType => _clip.ClipType;
        public string? SourceApp => _clip.SourceApp;
        public bool IsMultiLine => Content.Contains('\n', StringComparison.Ordinal);
        public bool WasTrimmed => _clip.WasTrimmed;
        public int Index
        {
            get => _index;
            set
            {
                if (SetProperty(ref _index, value))
                {
                    _ = LoadQuickPasteIconAsync();
                }
            }
        }
        public long SizeInBytes => _clip.SizeInBytes;
        public bool IsContentTruncated => _clip.Content == null;

        public string DisplayContent =>
            ClipType == AppConstants.ClipTypes.Link && !string.IsNullOrEmpty(SourceApp) && SourceApp.EndsWith(".url", StringComparison.OrdinalIgnoreCase)
                ? SourceApp
                : Content;
        public string? RtfContent => IsRtf ? Content : null;

        public bool CanPasteAsPlainText => IsRtf;
        public bool CanPasteAsRtf => MainViewModel.CurrentSettings.PasteAsPlainText && IsRtf;
        public bool IsEditable => !IsImage && !ClipType.StartsWith("file_", StringComparison.Ordinal) && ClipType != AppConstants.ClipTypes.Folder;
        public bool IsOpenable => !IsSourceMissing && (IsImage || ClipType.StartsWith("file_", StringComparison.Ordinal) || ClipType == AppConstants.ClipTypes.Folder || ClipType == AppConstants.ClipTypes.Link);
        public static string OpenCommandHeader => "Open";

        public bool IsFileBased => IsImage || ClipType.StartsWith("file_", StringComparison.Ordinal) || ClipType == AppConstants.ClipTypes.Folder;
        public string? FileProperties { get => _fileProperties; private set => SetProperty(ref _fileProperties, value); }
        public string? FileTypeInfo { get => _fileTypeInfo; private set => SetProperty(ref _fileTypeInfo, value); }
        public bool IsFilePropertiesLoading { get => _isFilePropertiesLoading; private set => SetProperty(ref _isFilePropertiesLoading, value); }
        public string? PageTitle { get => _pageTitle; private set => SetProperty(ref _pageTitle, value); }
        public bool IsPageTitleLoading { get => _isPageTitleLoading; private set => SetProperty(ref _isPageTitleLoading, value); }
        public ImageSource? ImagePreviewSource { get => _imagePreviewSource; private set => SetProperty(ref _imagePreviewSource, value); }
        public bool HasThumbnail { get => _hasThumbnail; private set => SetProperty(ref _hasThumbnail, value); }

        public FontFamily PreviewFont { get => _previewFont; set => SetProperty(ref _previewFont, value); }
        public double PreviewFontSize { get => _previewFontSize; set => SetProperty(ref _previewFontSize, value); }
        public uint HoverImagePreviewSize { get; set; }

        public bool IsComparable => ClipType is AppConstants.ClipTypes.Text or AppConstants.ClipTypes.CodeSnippet or AppConstants.ClipTypes.Rtf or AppConstants.ClipTypes.Dev or AppConstants.ClipTypes.FileText;
        public string? FileName => IsFileBased ? Path.GetFileName(Content.Trim()) : null;
        public string CompareLeftHeader { get => _compareLeftHeader; set => SetProperty(ref _compareLeftHeader, value); }
        public bool ShowCompareRightOption { get => _showCompareRightOption; set => SetProperty(ref _showCompareRightOption, value); }

        private string? _tooltipTextContent;
        public string? TooltipTextContent { get => _tooltipTextContent; private set => SetProperty(ref _tooltipTextContent, value); }

        private string? _lineCountInfo;
        public string? LineCountInfo { get => _lineCountInfo; private set => SetProperty(ref _lineCountInfo, value); }

        public bool IsPasteGroupVisible => IsEditable || IsRtf;

        public bool IsPinned { get => _isPinned; set => SetProperty(ref _isPinned, value); }

        public ImageSource? ThumbnailSource
        {
            get
            {
                if (_thumbnailSource == null && !_isThumbnailLoading)
                {
                    _ = LoadThumbnailAsync();
                }
                return _thumbnailSource;
            }
            private set => SetProperty(ref _thumbnailSource, value);
        }

        public string PaddingSize { get => _paddingSize; set => SetProperty(ref _paddingSize, value); }
        public string Preview { get; private set; } = string.Empty;

        public FontFamily CurrentFontFamily { get => _currentFontFamily; set => SetProperty(ref _currentFontFamily, value); }
        public double CurrentFontSize { get => _currentFontSize; set => SetProperty(ref _currentFontSize, value); }
        public ICommand TogglePinCommand { get; }
        public ICommand DeleteCommand { get; }
        public ICommand EditClipCommand { get; }
        public ICommand MoveToTopCommand { get; }
        public ICommand OpenCommand { get; }
        public ICommand SelectForCompareLeftCommand { get; }
        public ICommand CompareWithSelectedRightCommand { get; }
        public ICommand SendToCommand { get; }
        public ICommand TogglePreviewCommand { get; }
        public ClipViewModel(Clip clip, MainViewModel mainViewModel, IClipDetailsLoader clipDetailsLoader, IIconProvider iconProvider, IClipDataService clipDataService, IThumbnailService thumbnailService, IWebMetadataService webMetadataService)
        {
            ArgumentNullException.ThrowIfNull(clip);
            ArgumentNullException.ThrowIfNull(mainViewModel);

            _clip = clip;
            _clipDetailsLoader = clipDetailsLoader;
            _isPinned = clip.IsPinned;
            _paddingSize = mainViewModel.CurrentSettings.ClipItemPadding;
            MainViewModel = mainViewModel;
            _iconProvider = iconProvider;
            _clipDataService = clipDataService;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;

            TogglePinCommand = new RelayCommand(_ => MainViewModel.TogglePinCommand.Execute(this));
            DeleteCommand = new RelayCommand(_ => MainViewModel.DeleteClipCommand.Execute(this));
            EditClipCommand = new RelayCommand(_ => MainViewModel.EditClipCommand.Execute(this));
            MoveToTopCommand = new RelayCommand(_ => MainViewModel.MoveToTopCommand.Execute(this));
            OpenCommand = new RelayCommand(_ => MainViewModel.OpenCommand.Execute(this));
            SelectForCompareLeftCommand = new RelayCommand(_ => MainViewModel.SelectForCompareLeftCommand.Execute(this));
            CompareWithSelectedRightCommand = new RelayCommand(_ => MainViewModel.CompareWithSelectedRightCommand.Execute(this));
            SendToCommand = new RelayCommand(p => MainViewModel.SendToCommand.Execute(new object[] { this, p as SendToTarget ?? null! }));
            TogglePreviewCommand = new RelayCommand(p => ExecuteTogglePreview(p));
        }

        private void ExecuteTogglePreview(object? parameter)
        {
            if (parameter is UIElement placementTarget)
            {
                MainViewModel.TogglePreviewForSelection(placementTarget);
            }
        }

        internal async Task<Clip?> GetFullClipAsync()
        {
            var fullClip = await _clipDataService.GetClipByIdAsync(Id).ConfigureAwait(false);
            return fullClip;
        }

        public void UpdateClip(Clip clip, string theme)
        {
            ArgumentNullException.ThrowIfNull(clip);

            _clip = clip;
            _theme = theme;
            IsPinned = clip.IsPinned;
            ReleaseThumbnail();
            FileProperties = null;
            FileTypeInfo = null;
            PageTitle = null;
            IsSourceMissing = false;

            _ = UpdateSourceMissingStateAsync();

            UpdatePreviewText();
            ClearTooltipContent();
            _ = LoadIconsAsync();

            OnPropertyChanged(nameof(FileName));
            OnPropertyChanged(nameof(Content));
            OnPropertyChanged(nameof(DisplayContent));
            OnPropertyChanged(nameof(Timestamp));
            OnPropertyChanged(nameof(ClipType));
            OnPropertyChanged(nameof(IsImage));
            OnPropertyChanged(nameof(IsRtf));
            OnPropertyChanged(nameof(RtfContent));
            OnPropertyChanged(nameof(ShowTextualTooltip));
            OnPropertyChanged(nameof(IsMultiLine));
            OnPropertyChanged(nameof(WasTrimmed));
            OnPropertyChanged(nameof(IsPinned));
            OnPropertyChanged(nameof(IsFileBased));
            OnPropertyChanged(nameof(IsLinkToolTip));
            OnPropertyChanged(nameof(SizeInBytes));
            OnPropertyChanged(nameof(IsContentTruncated));
            OnPropertyChanged(nameof(IsOpenable));
            OnPropertyChanged(nameof(ShowCompareMenu));
        }

        private async Task UpdateSourceMissingStateAsync()
        {
            bool isMissing = false;
            if (IsFileBased && !string.IsNullOrEmpty(Content))
            {
                isMissing = await Task.Run(() =>
                {
                    var path = Content.Trim();
                    if (ClipType == AppConstants.ClipTypes.Folder)
                    {
                        return !Directory.Exists(path);
                    }
                    return !File.Exists(path);
                }).ConfigureAwait(false);
            }
            IsSourceMissing = isMissing;
        }

        private async Task LoadIconsAsync()
        {
            ClipTypeIcon = await _iconProvider.GetIconAsync(ClipType, 20).ConfigureAwait(true);
        }

        private async Task LoadQuickPasteIconAsync()
        {
            if (Index > 0 && Index <= 9)
            {
                QuickPasteIcon = await _iconProvider.GetIconAsync(Index.ToString(CultureInfo.InvariantCulture), 32).ConfigureAwait(true);
            }
            else
            {
                QuickPasteIcon = null;
            }
        }

        private void UpdatePreviewText()
        {
            string basePreview;
            var searchTerm = MainViewModel.SearchTerm;
            bool isSearching = !string.IsNullOrEmpty(searchTerm);
            const string startTag = "[HL]";
            const string endTag = "[/HL]";

            if (isSearching && !string.IsNullOrWhiteSpace(_clip.MatchContext) && _clip.MatchContext.Contains(startTag, StringComparison.Ordinal))
            {
                string context = _clip.MatchContext.ReplaceLineEndings(" ");
                int highlightStart = context.IndexOf(startTag, StringComparison.Ordinal);
                int highlightEnd = context.IndexOf(endTag, highlightStart, StringComparison.Ordinal);

                if (highlightStart != -1 && highlightEnd != -1)
                {
                    const int contextBefore = 10; // Show 10 chars before the highlight
                    const int contextAfter = 60;  // Show 60 chars after the highlight starts
                    int actualHighlightStart = highlightStart + startTag.Length;
                    int actualHighlightEnd = highlightEnd;
                    string contextWithoutTags = context.Remove(highlightEnd, endTag.Length).Remove(highlightStart, startTag.Length);
                    int adjustedHighlightStart = highlightStart;
                    int adjustedHighlightEnd = highlightEnd - startTag.Length;
                    int visibleStart = Math.Max(0, adjustedHighlightStart - contextBefore);
                    int visibleEnd = Math.Min(contextWithoutTags.Length, adjustedHighlightStart + contextAfter);

                    if (adjustedHighlightEnd > visibleEnd)
                    {
                        visibleEnd = Math.Min(contextWithoutTags.Length, adjustedHighlightEnd);
                    }

                    basePreview = contextWithoutTags.Substring(visibleStart, visibleEnd - visibleStart);
                    int previewHighlightStart = adjustedHighlightStart - visibleStart;
                    int previewHighlightEnd = adjustedHighlightEnd - visibleStart;

                    if (previewHighlightStart >= 0 && previewHighlightEnd <= basePreview.Length && previewHighlightStart < previewHighlightEnd)
                    {
                        string highlightedText = basePreview.Substring(previewHighlightStart, previewHighlightEnd - previewHighlightStart);
                        basePreview = basePreview.Remove(previewHighlightStart, highlightedText.Length);
                        basePreview = basePreview.Insert(previewHighlightStart, $"{startTag}{highlightedText}{endTag}");
                    }

                    if (visibleStart > 0)
                    {
                        basePreview = "..." + basePreview;
                    }
                    if (visibleEnd < contextWithoutTags.Length)
                    {
                        basePreview += "...";
                    }
                }
                else
                {
                    basePreview = _clip.MatchContext;
                }
            }
            else
            {
                if (_clip.ClipType == AppConstants.ClipTypes.Rtf)
                {
                    basePreview = RtfUtils.ToPlainText(Content);
                }
                else
                {
                    using (var reader = new StringReader(DisplayContent))
                    {
                        string? firstLine = string.Empty;
                        string? currentLine;
                        while ((currentLine = reader.ReadLine()) != null)
                        {
                            if (!string.IsNullOrWhiteSpace(currentLine))
                            {
                                firstLine = currentLine.Trim();
                                break;
                            }
                        }
                        basePreview = firstLine ?? string.Empty;
                    }
                }

                if (isSearching)
                {
                    var terms = searchTerm.Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries);
                    foreach (var term in terms)
                    {
                        try
                        {
                            basePreview = System.Text.RegularExpressions.Regex.Replace(basePreview, System.Text.RegularExpressions.Regex.Escape(term), "[HL]$0[/HL]", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
                        }
                        catch (ArgumentException) { }
                    }
                }
            }

            Preview = basePreview.Trim();
            OnPropertyChanged(nameof(Preview));
        }

        public void ReleaseThumbnail()
        {
            Interlocked.Increment(ref _currentThumbnailLoadId);
            ThumbnailSource = null;
            HasThumbnail = false;
            _isThumbnailLoading = false;
        }

        public async Task LoadThumbnailAsync()
        {
            if (_isThumbnailLoading) return;

            _isThumbnailLoading = true;
            try
            {
                var loadId = Interlocked.Increment(ref _currentThumbnailLoadId);
                string? newThumbnailPath = await _clipDetailsLoader.GetThumbnailAsync(this, _thumbnailService, _webMetadataService, _theme).ConfigureAwait(false);

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

                await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    ThumbnailSource = finalBitmap;
                    HasThumbnail = finalBitmap != null;
                });
            }
            finally
            {
                _isThumbnailLoading = false;
            }
        }

        public void NotifyPasteAsPropertiesChanged()
        {
            OnPropertyChanged(nameof(CanPasteAsPlainText));
            OnPropertyChanged(nameof(CanPasteAsRtf));
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _filePropertiesCts?.Dispose();
                    _pageTitleCts?.Dispose();
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