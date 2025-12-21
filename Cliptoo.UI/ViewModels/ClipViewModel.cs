using System.ComponentModel;
using System.Diagnostics.CodeAnalysis;
using System.Globalization;
using System.IO;
using System.Text;
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
    [SuppressMessage("Design", "CA1515:Consider making public types internal", Justification = "Used by XAML binding.")]
    public class ClipViewModel : ViewModelBase, IDisposable
    {
        private const int MaxTooltipLines = 40;
        private static readonly char[] _spaceSeparator = [' '];

        internal Clip _clip;
        private readonly IClipDataService _clipDataService;
        private readonly IClipDetailsLoader _clipDetailsLoader;
        private readonly IIconProvider _iconProvider;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;
        private readonly IEventAggregator _eventAggregator;
        private readonly IComparisonStateService _comparisonStateService;
        private readonly IPreviewManager _previewManager;
        private readonly IFontProvider _fontProvider;

        private readonly CancellationTokenSource _disposalCts = new();
        private CancellationTokenSource? _tooltipCts;
        private string _theme = "light";
        private bool _isFavorite;
        private int _index;
        private string _compareLeftHeader = "Compare Left";
        private bool _showCompareRightOption;
        private bool _disposedValue;
        private bool _isTooltipContentLoaded;
        private int _currentThumbnailLoadId;

        public ISettingsService SettingsService { get; }
        public IUiSharedResources SharedResources { get; }
        public Settings CurrentSettings => SettingsService.Settings;

        private FontFamily _mainFont = null!;
        public FontFamily MainFont { get => _mainFont; private set => SetProperty(ref _mainFont, value); }

        private FontFamily _previewFont = null!;
        public FontFamily PreviewFont { get => _previewFont; private set => SetProperty(ref _previewFont, value); }

        #region Clip Properties
        public int Id => _clip.Id;
        public string Content => _clip.PreviewContent ?? string.Empty;
        public DateTime Timestamp => _clip.Timestamp;
        public string ClipType => _clip.ClipType;
        public string? SourceApp => _clip.SourceApp;
        public bool IsMultiLine => Content.Contains('\n', StringComparison.Ordinal);
        public bool WasTrimmed => _clip.WasTrimmed;
        public long SizeInBytes => _clip.SizeInBytes;
        public string? Tags => _clip.Tags;
        public bool HasTags => !string.IsNullOrEmpty(Tags);
        public bool IsFavorite { get => _isFavorite; set => SetProperty(ref _isFavorite, value); }
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
        #endregion

        #region Async Loading Properties
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
            private set
            {
                if (SetProperty(ref _isSourceMissing, value))
                {
                    OnPropertyChanged(nameof(IsOpenable));
                    OnPropertyChanged(nameof(ShowCompareMenu));
                }
            }
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
        #endregion

        #region Logic Helpers
        public string DisplayContent =>
            ClipType == AppConstants.ClipTypeLink && !string.IsNullOrEmpty(SourceApp) && SourceApp.EndsWith(".url", StringComparison.OrdinalIgnoreCase)
                ? SourceApp
                : Content;

        public bool CanPasteAsPlainText => IsRtf;
        public bool CanPasteAsRtf => CurrentSettings.PasteAsPlainText && IsRtf;
        public bool IsEditable => ClipTypeHelper.IsEditable(ClipType);
        public bool IsOpenable => !IsSourceMissing && (IsImage || IsFileBased || ClipType == AppConstants.ClipTypeLink);
        public bool IsFileBased => ClipTypeHelper.IsFileBased(ClipType);
        public bool IsComparable => ClipTypeHelper.IsComparable(ClipType);
        public string? FileName => IsFileBased ? Path.GetFileName(Content.Trim()) : null;
        public string CompareLeftHeader { get => _compareLeftHeader; set => SetProperty(ref _compareLeftHeader, value); }
        public bool ShowCompareRightOption { get => _showCompareRightOption; set => SetProperty(ref _showCompareRightOption, value); }
        public bool ShowCompareMenu => !IsSourceMissing && IsComparable && _comparisonStateService.IsCompareToolAvailable;
        public bool IsPasteGroupVisible => IsEditable || IsRtf;
        public bool IsImage => ClipType == AppConstants.ClipTypeImage;
        public bool IsRtf => ClipType == AppConstants.ClipTypeRtf;
        public bool IsLinkToolTip => ClipType == AppConstants.ClipTypeLink;
        public bool IsPreviewableAsTextFile => ClipTypeHelper.IsPreviewableAsTextFile(ClipType, Content);
        public bool ShowFileInfoTooltip => IsFileBased && !IsImage && !IsPreviewableAsTextFile;
        public bool ShowTextualTooltip => IsPreviewableAsTextFile || (!IsFileBased && !IsLinkToolTip && ClipType != AppConstants.ClipTypeColor);
        public static string OpenCommandHeader => "Open";
        #endregion

        private bool _isDeleting;
        public bool IsDeleting { get => _isDeleting; set => SetProperty(ref _isDeleting, value); }

        private bool _isMoving;
        public bool IsMoving { get => _isMoving; set => SetProperty(ref _isMoving, value); }

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
        public ICommand PasteFilePathCommand { get; }

        public ClipViewModel(
            Clip clip,
            IClipDataService clipDataService, IClipDetailsLoader clipDetailsLoader, IIconProvider iconProvider, IThumbnailService thumbnailService, IWebMetadataService webMetadataService,
            IEventAggregator eventAggregator,
            IComparisonStateService comparisonStateService,
            ISettingsService settingsService,
            IUiSharedResources sharedResources,
            IFontProvider fontProvider,
            IPreviewManager previewManager)
        {
            _clip = clip;
            _clipDataService = clipDataService;
            _clipDetailsLoader = clipDetailsLoader;
            _iconProvider = iconProvider;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
            _eventAggregator = eventAggregator;
            _comparisonStateService = comparisonStateService;
            SettingsService = settingsService;
            SharedResources = sharedResources;
            _fontProvider = fontProvider;
            _previewManager = previewManager;

            _isFavorite = clip.IsFavorite;
            _mainFont = fontProvider.GetFont(CurrentSettings.FontFamily);
            _previewFont = fontProvider.GetFont(CurrentSettings.PreviewFontFamily);

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
            PasteFilePathCommand = new RelayCommand(_ => _eventAggregator.Publish(new ClipPasteFilePathRequested(Id)));
            TransformAndPasteCommand = new RelayCommand(p =>
            {
                if (p is object[] values && values.Length == 2 && values[1] is string transformType)
                {
                    _eventAggregator.Publish(new ClipTransformAndPasteRequested(Id, transformType));
                }
            });

            _eventAggregator.Subscribe<SettingsChangedMessage>(OnSettingsChanged);
        }

        private void OnSettingsChanged(SettingsChangedMessage msg)
        {
            switch (msg.PropertyName)
            {
                case nameof(Settings.FontFamily):
                    MainFont = _fontProvider.GetFont(CurrentSettings.FontFamily);
                    break;
                case nameof(Settings.PreviewFontFamily):
                    PreviewFont = _fontProvider.GetFont(CurrentSettings.PreviewFontFamily);
                    break;
                case nameof(Settings.AccentColor):
                case nameof(Settings.AccentChromaLevel):
                    NotifyAccentColorChanged();
                    break;
            }
        }

        private void CurrentSettings_PropertyChanged(object? sender, PropertyChangedEventArgs e)
        {
            switch (e.PropertyName)
            {
                case nameof(Settings.FontFamily):
                    MainFont = _fontProvider.GetFont(CurrentSettings.FontFamily);
                    break;
                case nameof(Settings.PreviewFontFamily):
                    PreviewFont = _fontProvider.GetFont(CurrentSettings.PreviewFontFamily);
                    break;
                case nameof(Settings.AccentColor):
                case nameof(Settings.AccentChromaLevel):
                    NotifyAccentColorChanged();
                    break;
            }
        }

        public void NotifyAccentColorChanged()
        {
            if (Index > 0) _ = LoadQuickPasteIconAsync();
        }

        public void UpdateClip(Clip clip, string theme)
        {
            _clip = clip;
            _theme = theme;
            IsFavorite = clip.IsFavorite;
            ReleaseThumbnail();
            ClearTooltipContent();
            _ = UpdateSourceMissingStateAsync();
            UpdatePreviewText();
            _ = LoadIconsAsync();

            OnPropertyChanged(string.Empty);
        }

        private void UpdatePreviewText()
        {
            string basePreview;
            var mainViewModel = (Application.Current.MainWindow?.DataContext as MainViewModel);
            var searchTerm = mainViewModel?.SearchTerm ?? string.Empty;
            bool isSearching = !string.IsNullOrEmpty(searchTerm);
            const string startTag = "[HL]";
            const string endTag = "[/HL]";

            var searchPrefix = CurrentSettings.TagSearchPrefix;
            string processedSearchTerm = searchTerm;
            if (isSearching && searchTerm.StartsWith(searchPrefix, StringComparison.Ordinal))
            {
                processedSearchTerm = searchTerm.Substring(searchPrefix.Length);
            }

            var highlightTerms = isSearching
                ? System.Text.RegularExpressions.Regex.Matches(processedSearchTerm, @"\w+")
                    .Cast<System.Text.RegularExpressions.Match>()
                    .Select(m => m.Value)
                    .Where(v => v.Length > 0)
                    .Distinct()
                    .ToList()
                : new List<string>();

            if (isSearching && !string.IsNullOrWhiteSpace(_clip.MatchContext) && _clip.MatchContext.Contains(startTag, StringComparison.Ordinal))
            {
                string context = _clip.MatchContext.ReplaceLineEndings(" ");
                string contextWithoutTags = context.Replace(startTag, "", StringComparison.Ordinal).Replace(endTag, "", StringComparison.Ordinal);
                int firstHighlightStart = context.IndexOf(startTag, StringComparison.Ordinal);
                int maxPreviewLength = mainViewModel?.MaxPreviewLength ?? 200;

                int idealStart = firstHighlightStart - (maxPreviewLength / 3);
                int idealEnd = idealStart + maxPreviewLength;

                if (idealStart < 0) { idealEnd += -idealStart; idealStart = 0; }
                if (idealEnd > contextWithoutTags.Length) { idealStart -= (idealEnd - contextWithoutTags.Length); idealEnd = contextWithoutTags.Length; }

                basePreview = contextWithoutTags.Substring(Math.Max(0, idealStart), Math.Max(0, Math.Min(contextWithoutTags.Length, idealEnd) - Math.Max(0, idealStart)));

                foreach (var term in highlightTerms)
                {
                    try { basePreview = System.Text.RegularExpressions.Regex.Replace(basePreview, $@"\b{System.Text.RegularExpressions.Regex.Escape(term)}", "[HL]$0[/HL]", System.Text.RegularExpressions.RegexOptions.IgnoreCase); }
                    catch (ArgumentException) { }
                }
            }
            else
            {
                string textForPreview = DisplayContent;
                using (var reader = new StringReader(textForPreview))
                {
                    string? firstLine = string.Empty;
                    string? currentLine;
                    while ((currentLine = reader.ReadLine()) != null)
                    {
                        if (!string.IsNullOrWhiteSpace(currentLine)) { firstLine = currentLine.Trim(); break; }
                    }
                    basePreview = firstLine ?? string.Empty;
                }

                if (isSearching)
                {
                    foreach (var term in highlightTerms)
                    {
                        try { basePreview = System.Text.RegularExpressions.Regex.Replace(basePreview, System.Text.RegularExpressions.Regex.Escape(term), "[HL]$0[/HL]", System.Text.RegularExpressions.RegexOptions.IgnoreCase); }
                        catch (ArgumentException) { }
                    }
                }
            }

            Preview = basePreview.Trim();
            OnPropertyChanged(nameof(Preview));
        }

        #region Async Fetching Logic
        public async Task LoadThumbnailAsync()
        {
            if (ThumbnailSource != null || IsThumbnailLoading || _disposedValue) return;

            IsThumbnailLoading = true;
            try
            {
                var loadId = Interlocked.Increment(ref _currentThumbnailLoadId);
                string? newThumbnailPath = await _clipDetailsLoader.GetThumbnailAsync(this, _thumbnailService, _webMetadataService, _theme).ConfigureAwait(false);

                if (loadId != _currentThumbnailLoadId || _disposedValue) return;

                if (!string.IsNullOrEmpty(newThumbnailPath))
                {
                    byte[]? bytes = null;
                    try
                    {
                        for (int i = 0; i < 3; i++)
                        {
                            try { bytes = await File.ReadAllBytesAsync(newThumbnailPath, _disposalCts.Token).ConfigureAwait(false); break; }
                            catch (IOException) { await Task.Delay(50).ConfigureAwait(false); }
                        }
                    }
                    catch (Exception) { }

                    if (bytes != null && loadId == _currentThumbnailLoadId)
                    {
                        await Application.Current.Dispatcher.InvokeAsync(() =>
                        {
                            if (loadId != _currentThumbnailLoadId || _disposedValue) return;
                            try
                            {
                                using var ms = new MemoryStream(bytes);
                                var bitmapImage = new BitmapImage();
                                bitmapImage.BeginInit();
                                bitmapImage.StreamSource = ms;
                                bitmapImage.CacheOption = BitmapCacheOption.OnLoad;
                                bitmapImage.EndInit();
                                bitmapImage.Freeze();
                                ThumbnailSource = bitmapImage;
                                HasThumbnail = true;
                            }
                            catch { ThumbnailSource = null; HasThumbnail = false; }
                        });
                    }
                }
            }
            finally { IsThumbnailLoading = false; }
        }

        public async Task LoadTooltipContentAsync()
        {
            if (_isTooltipContentLoaded || _disposedValue) return;
            _isTooltipContentLoaded = true;

            _tooltipCts?.Cancel();
            _tooltipCts = CancellationTokenSource.CreateLinkedTokenSource(_disposalCts.Token);
            var token = _tooltipCts.Token;

            var fullClip = await _clipDataService.GetClipByIdAsync(Id).ConfigureAwait(false);
            if (fullClip is null || token.IsCancellationRequested) return;

            string? textFileContent = null;
            if (IsPreviewableAsTextFile)
            {
                try
                {
                    if (File.Exists(Content))
                    {
                        using var reader = new StreamReader(Content, true);
                        var buffer = new char[4096];
                        int charsRead = await reader.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false);
                        textFileContent = new string(buffer, 0, charsRead);
                    }
                }
                catch (IOException) { }
            }

            await Task.WhenAll(LoadFilePropertiesAsync(token), LoadPageTitleAsync(token)).ConfigureAwait(false);

            if (!token.IsCancellationRequested)
            {
                await Application.Current.Dispatcher.InvokeAsync(() => GenerateTooltipProperties(fullClip, textFileContent));
            }
        }

        public void ClearTooltipContent()
        {
            _isTooltipContentLoaded = false;
            _tooltipCts?.Cancel();
            _tooltipCts = null;

            TooltipTextContent = null;
            LineCountInfo = null;
            ImagePreviewSource = null;
            PageTitle = null;
            FileProperties = null;
            FileTypeInfo = null;
            FileTypeInfoIcon = null;
            IsPageTitleLoading = false;
            IsFilePropertiesLoading = false;
        }

        private async Task LoadFilePropertiesAsync(CancellationToken token)
        {
            if (IsFilePropertiesLoading || !string.IsNullOrEmpty(FileProperties)) return;
            await Application.Current.Dispatcher.InvokeAsync(() => IsFilePropertiesLoading = true);

            var (properties, typeInfo, isMissing) = await _clipDetailsLoader.GetFilePropertiesAsync(this, token).ConfigureAwait(false);
            if (token.IsCancellationRequested) return;

            ImageSource? typeIcon = !isMissing && !string.IsNullOrEmpty(ClipType) ? await _iconProvider.GetIconAsync(ClipType, 16) : null;

            await Application.Current.Dispatcher.InvokeAsync(() =>
            {
                if (token.IsCancellationRequested) return;
                FileProperties = properties;
                FileTypeInfo = typeInfo;
                IsSourceMissing = isMissing;
                FileTypeInfoIcon = typeIcon;
                IsFilePropertiesLoading = false;
            });
        }

        private async Task LoadPageTitleAsync(CancellationToken token)
        {
            if (IsPageTitleLoading || !string.IsNullOrEmpty(PageTitle) || !IsLinkToolTip) return;
            await Application.Current.Dispatcher.InvokeAsync(() => IsPageTitleLoading = true);

            var title = await _clipDetailsLoader.GetPageTitleAsync(this, _webMetadataService, token).ConfigureAwait(false);
            await Application.Current.Dispatcher.InvokeAsync(() =>
            {
                if (token.IsCancellationRequested) return;
                PageTitle = title;
                IsPageTitleLoading = false;
            });
        }

        public async Task LoadImagePreviewAsync(uint largestDimension)
        {
            if (!IsImage || _disposedValue) return;
            var token = _tooltipCts?.Token ?? _disposalCts.Token;

            var path = await _clipDetailsLoader.GetImagePreviewAsync(this, _thumbnailService, largestDimension, _theme).ConfigureAwait(false);
            if (string.IsNullOrEmpty(path) || token.IsCancellationRequested) return;

            try
            {
                var bytes = await File.ReadAllBytesAsync(path, token).ConfigureAwait(false);
                await Application.Current.Dispatcher.InvokeAsync(() =>
                {
                    if (token.IsCancellationRequested) return;
                    try
                    {
                        using var ms = new MemoryStream(bytes);
                        var bitmap = new BitmapImage();
                        bitmap.BeginInit(); bitmap.StreamSource = ms; bitmap.CacheOption = BitmapCacheOption.OnLoad; bitmap.EndInit();
                        bitmap.Freeze();
                        ImagePreviewSource = bitmap;
                    }
                    catch { ImagePreviewSource = null; }
                });
            }
            catch (OperationCanceledException) { }
        }

        private async Task LoadIconsAsync()
        {
            var icon = await _iconProvider.GetIconAsync(ClipType, 20);
            await Application.Current.Dispatcher.InvokeAsync(() => ClipTypeIcon = icon);
        }

        private async Task LoadQuickPasteIconAsync()
        {
            if (Index > 0 && Index <= 9)
            {
                var icon = await _iconProvider.GetIconAsync(Index.ToString(CultureInfo.InvariantCulture), 32);
                await Application.Current.Dispatcher.InvokeAsync(() => QuickPasteIcon = icon);
            }
            else await Application.Current.Dispatcher.InvokeAsync(() => QuickPasteIcon = null);
        }

        private async Task UpdateSourceMissingStateAsync()
        {
            bool isMissing = false;
            if (IsFileBased && !string.IsNullOrEmpty(Content))
            {
                isMissing = await Task.Run(() =>
                {
                    var path = Content.Trim();
                    return ClipType == AppConstants.ClipTypeFolder ? !Directory.Exists(path) : !File.Exists(path);
                }, _disposalCts.Token).ConfigureAwait(false);
            }
            await Application.Current.Dispatcher.InvokeAsync(() => IsSourceMissing = isMissing);
        }

        private void GenerateTooltipProperties(Clip clip, string? textFileContent)
        {
            if (IsImage) return;
            string content = textFileContent ?? (clip.ClipType == AppConstants.ClipTypeRtf ? Core.Services.RtfUtils.ToPlainText(clip.Content ?? "") : clip.Content ?? "");

            const int MaxChars = 16 * 1024;
            bool truncated = content.Length > MaxChars;
            if (truncated) content = content.Substring(0, MaxChars);

            if (ShowTextualTooltip && !string.IsNullOrEmpty(content))
            {
                var sb = new StringBuilder();
                int totalLines = 0, processed = 0;
                using var reader = new StringReader(content);
                string? line;
                while ((line = reader.ReadLine()) != null)
                {
                    totalLines++;
                    if (processed < MaxTooltipLines) { processed++; sb.AppendLine(line); }
                }
                if (totalLines == 0 && content.Length > 0) totalLines = 1;

                var final = new StringBuilder();
                var lines = sb.ToString().Split([Environment.NewLine], StringSplitOptions.None);
                int pad = totalLines.ToString(CultureInfo.InvariantCulture).Length;
                for (int i = 0; i < processed; i++) final.AppendLine(CultureInfo.InvariantCulture, $"{(i + 1).ToString(CultureInfo.InvariantCulture).PadLeft(pad)} | {lines[i]}");

                if (!IsFileBased) LineCountInfo = $"Size: {FormatUtils.FormatBytes(SizeInBytes)}{(totalLines > 1 ? $", {totalLines} lines" : "")}";
                if (totalLines > MaxTooltipLines || truncated) final.AppendLine(CultureInfo.InvariantCulture, $"\n... (truncated)");
                TooltipTextContent = final.ToString();
            }
        }
        #endregion

        public async Task<Clip?> GetFullClipAsync() => await _clipDataService.GetClipByIdAsync(Id).ConfigureAwait(false);

        public void RequestShowPreview() => _previewManager.RequestShowPreview(this);
        public void RequestHidePreview() => _previewManager.RequestHidePreview();
        public void ReleaseThumbnail() { _currentThumbnailLoadId++; ThumbnailSource = null; HasThumbnail = false; IsThumbnailLoading = false; }
        public void NotifyPasteAsPropertiesChanged() { OnPropertyChanged(nameof(CanPasteAsPlainText)); OnPropertyChanged(nameof(CanPasteAsRtf)); }

        public void Dispose()
        {
            if (!_disposedValue)
            {
                _disposalCts.Cancel();
                _disposalCts.Dispose();
                _tooltipCts?.Dispose();
                _disposedValue = true;
            }
            GC.SuppressFinalize(this);
        }
    }
}
