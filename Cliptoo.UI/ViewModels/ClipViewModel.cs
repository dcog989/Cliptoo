using System.ComponentModel;
using System.IO;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
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
    public class ClipViewModel : ViewModelBase, IDisposable
    {
        internal Clip _clip;
        private readonly IClipDataService _clipDataService;
        private readonly IEventAggregator _eventAggregator;
        private readonly IComparisonStateService _comparisonStateService;
        private readonly IPreviewManager _previewManager;
        private readonly ClipViewModelDetails _details;
        private bool _isFavorite;
        private int _index;
        private string _compareLeftHeader = "Compare Left";
        private bool _showCompareRightOption;
        private bool _disposedValue;
        private static readonly char[] _spaceSeparator = [' '];

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
                    _ = _details.LoadQuickPasteIconAsync();
                }
            }
        }
        public long SizeInBytes => _clip.SizeInBytes;
        public bool IsContentTruncated => _clip.Content == null;
        public string? Tags => _clip.Tags;
        public bool HasTags => !string.IsNullOrEmpty(Tags);

        public string DisplayContent =>
            ClipType == AppConstants.ClipTypeLink && !string.IsNullOrEmpty(SourceApp) && SourceApp.EndsWith(".url", StringComparison.OrdinalIgnoreCase)
                ? SourceApp
                : Content;
        public string? RtfContent => IsRtf ? Content : null;

        public bool CanPasteAsPlainText => IsRtf;
        public bool CanPasteAsRtf => CurrentSettings.PasteAsPlainText && IsRtf;
        public bool IsEditable => !IsImage && !ClipType.StartsWith("file_", StringComparison.Ordinal) && ClipType != AppConstants.ClipTypeFolder;
        public bool IsOpenable => !_details.IsSourceMissing && (IsImage || ClipType.StartsWith("file_", StringComparison.Ordinal) || ClipType == AppConstants.ClipTypeFolder || ClipType == AppConstants.ClipTypeLink);
        public static string OpenCommandHeader => "Open";

        public bool IsFileBased => IsImage || ClipType.StartsWith("file_", StringComparison.Ordinal) || ClipType == AppConstants.ClipTypeFolder;
        public bool IsComparable => ClipType is AppConstants.ClipTypeText or AppConstants.ClipTypeCodeSnippet or AppConstants.ClipTypeRtf or AppConstants.ClipTypeDev or AppConstants.ClipTypeFileText;
        public string? FileName => IsFileBased ? Path.GetFileName(Content.Trim()) : null;
        public string CompareLeftHeader { get => _compareLeftHeader; set => SetProperty(ref _compareLeftHeader, value); }
        public bool ShowCompareRightOption { get => _showCompareRightOption; set => SetProperty(ref _showCompareRightOption, value); }
        public bool ShowCompareMenu => !IsSourceMissing && IsComparable && _comparisonStateService.IsCompareToolAvailable;

        public bool IsPasteGroupVisible => IsEditable || IsRtf;
        public bool IsFavorite { get => _isFavorite; set => SetProperty(ref _isFavorite, value); }

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
            ArgumentNullException.ThrowIfNull(clip);
            ArgumentNullException.ThrowIfNull(fontProvider);
            _clip = clip;
            _details = new ClipViewModelDetails(this, clipDataService, clipDetailsLoader, thumbnailService, webMetadataService, iconProvider);
            _details.PropertyChanged += OnDetailsPropertyChanged;
            _isFavorite = clip.IsFavorite;
            _clipDataService = clipDataService;
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

        #region Delegated Properties
        public ImageSource? ThumbnailSource => _details.ThumbnailSource;
        public bool HasThumbnail => _details.HasThumbnail;
        public ImageSource? ClipTypeIcon => _details.ClipTypeIcon;
        public ImageSource? QuickPasteIcon => _details.QuickPasteIcon;
        public ImageSource? ImagePreviewSource => _details.ImagePreviewSource;
        public string? FileProperties => _details.FileProperties;
        public string? FileTypeInfo => _details.FileTypeInfo;
        public bool IsFilePropertiesLoading => _details.IsFilePropertiesLoading;
        public bool IsSourceMissing => _details.IsSourceMissing;
        public ImageSource? FileTypeInfoIcon => _details.FileTypeInfoIcon;
        public string? PageTitle => _details.PageTitle;
        public bool IsPageTitleLoading => _details.IsPageTitleLoading;
        public string? TooltipTextContent => _details.TooltipTextContent;
        public string? LineCountInfo => _details.LineCountInfo;
        #endregion

        #region Tooltip Logic
        public bool IsImage => ClipType == AppConstants.ClipTypeImage;
        public bool IsRtf => ClipType == AppConstants.ClipTypeRtf;
        public bool IsLinkToolTip => ClipType == AppConstants.ClipTypeLink;
        public bool IsPreviewableAsTextFile =>
            (ClipType is AppConstants.ClipTypeFileText or AppConstants.ClipTypeDev ||
            (ClipType == AppConstants.ClipTypeDocument &&
            (Content.EndsWith(".md", StringComparison.OrdinalIgnoreCase) ||
                Content.EndsWith(".markdown", StringComparison.OrdinalIgnoreCase) ||
                Content.EndsWith(".csv", StringComparison.OrdinalIgnoreCase))))
            && !string.Equals(Content, LogManager.LogFilePath, StringComparison.OrdinalIgnoreCase);

        public bool ShowFileInfoTooltip => IsFileBased && !IsImage && !IsPreviewableAsTextFile;
        public bool ShowTextualTooltip => IsPreviewableAsTextFile || (!IsFileBased && !IsLinkToolTip && ClipType != AppConstants.ClipTypeColor);
        #endregion

        private void OnDetailsPropertyChanged(object? sender, PropertyChangedEventArgs e)
        {
            if (e.PropertyName == nameof(ClipViewModelDetails.IsSourceMissing))
            {
                OnPropertyChanged(nameof(IsOpenable));
                OnPropertyChanged(nameof(ShowCompareMenu));
            }
            OnPropertyChanged(e.PropertyName);
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
            _details.UpdateTheme(theme);
            IsFavorite = clip.IsFavorite;
            ReleaseThumbnail();
            _details.ClearTooltipContent();
            _ = UpdateSourceMissingStateAsync();

            UpdatePreviewText();
            _ = _details.LoadIconsAsync();

            OnPropertyChanged(nameof(FileName));
            OnPropertyChanged(nameof(Content));
            OnPropertyChanged(nameof(DisplayContent));
            OnPropertyChanged(nameof(Timestamp));
            OnPropertyChanged(nameof(ClipType));
            OnPropertyChanged(nameof(Tags));
            OnPropertyChanged(nameof(HasTags));
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
        }

        private async Task UpdateSourceMissingStateAsync()
        {
            await _details.UpdateSourceMissingStateAsync();
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
                var searchPrefix = CurrentSettings.TagSearchPrefix;
                if (searchTerm.StartsWith(searchPrefix, StringComparison.Ordinal))
                {
                    terms = searchTerm.Substring(searchPrefix.Length).Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries);
                }

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
                if (_clip.ClipType == AppConstants.ClipTypeRtf)
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
                    var searchPrefix = CurrentSettings.TagSearchPrefix;
                    if (searchTerm.StartsWith(searchPrefix, StringComparison.Ordinal))
                    {
                        terms = searchTerm.Substring(searchPrefix.Length).Split(_spaceSeparator, StringSplitOptions.RemoveEmptyEntries);
                    }
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
            _details.ReleaseThumbnail();
        }

        public void NotifyPasteAsPropertiesChanged()
        {
            OnPropertyChanged(nameof(CanPasteAsPlainText));
            OnPropertyChanged(nameof(CanPasteAsRtf));
        }

        public Task LoadTooltipContentAsync() => _details.LoadTooltipContentAsync();
        public void ClearTooltipContent() => _details.ClearTooltipContent();
        public Task LoadImagePreviewAsync(uint size) => _details.LoadImagePreviewAsync(size);

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _details.PropertyChanged -= OnDetailsPropertyChanged;
                    _details.Dispose();
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