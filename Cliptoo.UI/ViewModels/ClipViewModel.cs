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
        private readonly IEventAggregator _eventAggregator;
        private readonly IComparisonStateService _comparisonStateService;
        private readonly IPreviewManager _previewManager;
        private ImageSource? _thumbnailSource;
        private bool _isThumbnailLoading;
        private bool _isFavorite;
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
        public bool IsCompareToolAvailable => _comparisonStateService.IsCompareToolAvailable;
        public bool ShowCompareMenu => !IsSourceMissing && IsComparable && IsCompareToolAvailable;
        public ISettingsService SettingsService { get; }
        public IUiSharedResources SharedResources { get; }
        public FontFamily MainFont { get; }
        public FontFamily PreviewFont { get; }
        public Settings CurrentSettings => SettingsService.Settings;

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
        public bool CanPasteAsRtf => CurrentSettings.PasteAsPlainText && IsRtf;
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

        public bool IsComparable => ClipType is AppConstants.ClipTypes.Text or AppConstants.ClipTypes.CodeSnippet or AppConstants.ClipTypes.Rtf or AppConstants.ClipTypes.Dev or AppConstants.ClipTypes.FileText;
        public string? FileName => IsFileBased ? Path.GetFileName(Content.Trim()) : null;
        public string CompareLeftHeader { get => _compareLeftHeader; set => SetProperty(ref _compareLeftHeader, value); }
        public bool ShowCompareRightOption { get => _showCompareRightOption; set => SetProperty(ref _showCompareRightOption, value); }

        private string? _tooltipTextContent;
        public string? TooltipTextContent { get => _tooltipTextContent; private set => SetProperty(ref _tooltipTextContent, value); }

        private string? _lineCountInfo;
        public string? LineCountInfo { get => _lineCountInfo; private set => SetProperty(ref _lineCountInfo, value); }

        public bool IsPasteGroupVisible => IsEditable || IsRtf;

        public bool IsFavorite { get => _isFavorite; set => SetProperty(ref _isFavorite, value); }

        private bool _isDeleting;
        public bool IsDeleting { get => _isDeleting; set => SetProperty(ref _isDeleting, value); }

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

        public string Preview { get; private set; } = string.Empty;

        public ICommand ToggleFavoriteCommand { get; }
        public ICommand DeleteCommand { get; }
        public ICommand EditClipCommand { get; }
        public ICommand MoveToTopCommand { get; }
        public ICommand OpenCommand { get; }
        public ICommand SelectForCompareLeftCommand { get; }
        public ICommand CompareWithSelectedRightCommand { get; }
        public ICommand SendToCommand { get; }
        public ICommand TogglePreviewCommand { get; }
        public ICommand PasteCommand { get; }
        public ICommand PasteAsPlainTextCommand { get; }
        public ICommand TransformAndPasteCommand { get; }

        public ClipViewModel(
            Clip clip,
            IClipDetailsLoader clipDetailsLoader,
            IIconProvider iconProvider,
            IClipDataService clipDataService,
            IThumbnailService thumbnailService,
            IWebMetadataService webMetadataService,
            IEventAggregator eventAggregator,
            IComparisonStateService comparisonStateService,
            ISettingsService settingsService,
            IUiSharedResources sharedResources,
            IFontProvider fontProvider,
            IPreviewManager previewManager)
        {
            ArgumentNullException.ThrowIfNull(clip);
            ArgumentNullException.ThrowIfNull(fontProvider);
            _clip = clip;
            _clipDetailsLoader = clipDetailsLoader;
            _isFavorite = clip.IsFavorite;
            _iconProvider = iconProvider;
            _clipDataService = clipDataService;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
            _eventAggregator = eventAggregator;
            _comparisonStateService = comparisonStateService;
            SettingsService = settingsService;
            SharedResources = sharedResources;
            _previewManager = previewManager;
            MainFont = fontProvider.GetFont(CurrentSettings.FontFamily);
            PreviewFont = fontProvider.GetFont(CurrentSettings.PreviewFontFamily);

            ToggleFavoriteCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipFavoriteToggled(Id, !IsFavorite)));
            DeleteCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipDeletionRequested(Id)));
            EditClipCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipEditRequested(Id)));
            MoveToTopCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipMoveToTopRequested(Id)));
            OpenCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipOpenRequested(Id)));
            SelectForCompareLeftCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipSelectForCompareLeft(Id)));
            CompareWithSelectedRightCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipCompareWithSelectedRight(Id)));
            SendToCommand = new RelayCommand(p => _eventAggregator.Publish(new ClipSendToRequested(Id, p as SendToTarget ?? null!)));
            TogglePreviewCommand = new RelayCommand(p => _eventAggregator.Publish(new TogglePreviewForSelectionRequested(p)));
            PasteCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipPasteRequested(Id, null)));
            PasteAsPlainTextCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipPasteRequested(Id, true)));
            TransformAndPasteCommand = new RelayCommand(p =>
            {
                if (p is object[] values && values.Length == 2 && values[1] is string transformType)
                {
                    _eventAggregator.Publish(new ClipTransformAndPasteRequested(Id, transformType));
                }
            });
        }

        public void RequestShowPreview() => _previewManager.RequestShowPreview(this);
        public void RequestHidePreview() => _previewManager.RequestHidePreview();

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
            IsFavorite = clip.IsFavorite;
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
            OnPropertyChanged(nameof(IsFavorite));
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
            var searchTerm = (Application.Current.MainWindow?.DataContext as MainViewModel)?.SearchTerm ?? string.Empty;
            bool isSearching = !string.IsNullOrEmpty(searchTerm);
            const string startTag = "[HL]";
            const string endTag = "[/HL]";

            if (isSearching && !string.IsNullOrWhiteSpace(_clip.MatchContext) && _clip.MatchContext.Contains(startTag, StringComparison.Ordinal))
            {
                string context = _clip.MatchContext.ReplaceLineEndings(" ");

                string contextWithoutTags = context.Replace(startTag, "", StringComparison.Ordinal).Replace(endTag, "", StringComparison.Ordinal);

                int firstHighlightStart = context.IndexOf(startTag, StringComparison.Ordinal);

                double windowWidth = CurrentSettings.WindowWidth;
                double fontSize = CurrentSettings.FontSize;
                double fixedWidth = 100;
                double avgCharWidthFactor = CurrentSettings.FontFamily == "Source Code Pro" ? 0.6 : 0.55;
                int maxPreviewLength = (int)((windowWidth - fixedWidth) / (fontSize * avgCharWidthFactor));
                if (maxPreviewLength < 40) maxPreviewLength = 40;

                int idealStart = firstHighlightStart - (maxPreviewLength / 3);
                int idealEnd = idealStart + maxPreviewLength;

                if (idealStart < 0)
                {
                    idealEnd += -idealStart;
                    idealStart = 0;
                }
                if (idealEnd > contextWithoutTags.Length)
                {
                    idealStart -= (idealEnd - contextWithoutTags.Length);
                    idealEnd = contextWithoutTags.Length;
                }

                int visibleStart = Math.Max(0, idealStart);
                int visibleEnd = Math.Min(contextWithoutTags.Length, idealEnd);
                int visibleLength = Math.Max(0, visibleEnd - visibleStart);
                basePreview = contextWithoutTags.Substring(visibleStart, visibleLength);

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