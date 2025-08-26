using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{

    public class ClipViewModel : ViewModelBase
    {
        private const int MaxTooltipLines = 40;

        private Clip _clip;
        public CliptooController Controller { get; }
        private readonly IPastingService _pastingService;
        private readonly INotificationService _notificationService;
        private readonly IClipDetailsLoader _clipDetailsLoader;
        private readonly IIconProvider _iconProvider;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;
        private ImageSource? _thumbnailSource;
        private bool _isPinned;
        private FontFamily _currentFontFamily = new("Segoe UI");
        private double _currentFontSize = 14;
        private string _paddingSize;
        private int _index;
        private string? _imagePreviewPath;
        private int _currentThumbnailLoadId;
        private bool _hasThumbnail;
        private string _theme = "light";
        private string? _fileProperties;
        private string? _fileTypeInfo;
        private bool _isFilePropertiesLoading;
        private bool _isSourceMissing;
        public bool IsSourceMissing { get => _isSourceMissing; private set => SetProperty(ref _isSourceMissing, value); }
        private CancellationTokenSource? _filePropertiesCts;
        private string? _pageTitle;
        private bool _isPageTitleLoading;
        private CancellationTokenSource? _pageTitleCts;
        private bool _isTooltipContentLoaded;
        private FontFamily _previewFont = new("Segoe UI");
        private double _previewFontSize = 14;
        private string _compareLeftHeader = "Compare Left";
        private bool _showCompareRightOption = false;
        private ImageSource? _clipTypeIcon;
        private ImageSource? _quickPasteIcon;
        private ImageSource? _fileTypeInfoIcon;
        public ImageSource? ClipTypeIcon { get => _clipTypeIcon; private set => SetProperty(ref _clipTypeIcon, value); }
        public ImageSource? QuickPasteIcon { get => _quickPasteIcon; private set => SetProperty(ref _quickPasteIcon, value); }
        public ImageSource? FileTypeInfoIcon { get => _fileTypeInfoIcon; private set => SetProperty(ref _fileTypeInfoIcon, value); }
        public bool IsTextTransformable => IsEditable;
        public bool IsCompareToolAvailable => MainViewModel.IsCompareToolAvailable;
        public bool ShowCompareMenu => IsComparable && IsCompareToolAvailable;
        public MainViewModel MainViewModel { get; }

        public int Id => _clip.Id;
        public string Content => _clip.PreviewContent ?? string.Empty;
        public DateTime Timestamp => _clip.Timestamp;
        public string ClipType => _clip.ClipType;
        public string? SourceApp => _clip.SourceApp;
        public bool IsMultiLine => Content.Contains('\n');
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

        public bool IsImage => ClipType == AppConstants.ClipTypes.Image;
        public bool IsRtf => ClipType == AppConstants.ClipTypes.Rtf;
        public string? RtfContent => IsRtf ? Content : null;
        public bool IsLinkToolTip => ClipType == AppConstants.ClipTypes.Link;

        public bool IsPreviewableAsTextFile =>
            ClipType is AppConstants.ClipTypes.FileText or AppConstants.ClipTypes.Dev ||
            (ClipType == AppConstants.ClipTypes.Document &&
            (Content.EndsWith(".md", StringComparison.OrdinalIgnoreCase) ||
                Content.EndsWith(".markdown", StringComparison.OrdinalIgnoreCase) ||
                Content.EndsWith(".csv", StringComparison.OrdinalIgnoreCase)));

        public bool ShowFileInfoTooltip => IsFileBased && !IsImage && !IsPreviewableAsTextFile;
        public bool ShowTextualTooltip => IsPreviewableAsTextFile || (!IsFileBased && !IsLinkToolTip && ClipType != AppConstants.ClipTypes.Color);

        public bool CanPasteAsPlainText => IsRtf;
        public bool CanPasteAsRtf => Controller.GetSettings().PasteAsPlainText && IsRtf;
        public bool IsEditable => !IsImage && !ClipType.StartsWith("file_", StringComparison.Ordinal) && ClipType != AppConstants.ClipTypes.Folder;
        public bool IsOpenable => IsImage || ClipType.StartsWith("file_", StringComparison.Ordinal) || ClipType == AppConstants.ClipTypes.Folder || ClipType == AppConstants.ClipTypes.Link;
        public string OpenCommandHeader => "Open";

        public bool IsFileBased => IsImage || ClipType.StartsWith("file_", StringComparison.Ordinal) || ClipType == AppConstants.ClipTypes.Folder;
        public string? FileProperties { get => _fileProperties; private set => SetProperty(ref _fileProperties, value); }
        public string? FileTypeInfo { get => _fileTypeInfo; private set => SetProperty(ref _fileTypeInfo, value); }
        public bool IsFilePropertiesLoading { get => _isFilePropertiesLoading; private set => SetProperty(ref _isFilePropertiesLoading, value); }
        public string? PageTitle { get => _pageTitle; private set => SetProperty(ref _pageTitle, value); }
        public bool IsPageTitleLoading { get => _isPageTitleLoading; private set => SetProperty(ref _isPageTitleLoading, value); }
        public string? ImagePreviewPath { get => _imagePreviewPath; private set => SetProperty(ref _imagePreviewPath, value); }
        public bool HasThumbnail { get => _hasThumbnail; private set => SetProperty(ref _hasThumbnail, value); }

        public FontFamily PreviewFont { get => _previewFont; set => SetProperty(ref _previewFont, value); }
        public double PreviewFontSize { get => _previewFontSize; set => SetProperty(ref _previewFontSize, value); }
        public uint HoverImagePreviewSize { get; set; }

        public bool IsComparable => ClipType is AppConstants.ClipTypes.Text or AppConstants.ClipTypes.CodeSnippet or AppConstants.ClipTypes.Rtf or AppConstants.ClipTypes.Dev or AppConstants.ClipTypes.FileText;
        public string? FileName => IsFileBased ? Path.GetFileName(Content.Trim()) : null;
        public string CompareLeftHeader { get => _compareLeftHeader; set => SetProperty(ref _compareLeftHeader, value); }
        public bool ShowCompareRightOption { get => _showCompareRightOption; set => SetProperty(ref _showCompareRightOption, value); }

        public string? TooltipTextContent { get; set; }
        public string? LineCountInfo { get; set; }

        public bool IsPinned { get => _isPinned; set => SetProperty(ref _isPinned, value); }
        public ImageSource? ThumbnailSource { get => _thumbnailSource; private set => SetProperty(ref _thumbnailSource, value); }
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

        public ICommand PasteAsPlainTextCommand { get; }
        public ICommand PasteAsRtfCommand { get; }
        public ICommand TransformAndPasteCommand { get; }
        public ICommand SendToCommand { get; }

        public ClipViewModel(Clip clip, CliptooController controller, IPastingService pastingService, INotificationService notificationService, IClipDetailsLoader clipDetailsLoader, string paddingSize, MainViewModel mainViewModel, IIconProvider iconProvider, IThumbnailService thumbnailService, IWebMetadataService webMetadataService)
        {
            _clip = clip;
            Controller = controller;
            _pastingService = pastingService;
            _notificationService = notificationService;
            _clipDetailsLoader = clipDetailsLoader;
            _isPinned = clip.IsPinned;
            _paddingSize = paddingSize;
            MainViewModel = mainViewModel;
            _iconProvider = iconProvider;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;

            TogglePinCommand = new RelayCommand(async _ => await TogglePinAsync());
            DeleteCommand = new RelayCommand(async _ => await DeleteAsync());
            EditClipCommand = new RelayCommand(_ => MainViewModel.HandleClipEdit(this));
            MoveToTopCommand = new RelayCommand(async _ => await MainViewModel.HandleClipMoveToTop(this));
            OpenCommand = new RelayCommand(async _ => await ExecuteOpen());
            SelectForCompareLeftCommand = new RelayCommand(_ => MainViewModel.HandleClipSelectForCompare(this));
            CompareWithSelectedRightCommand = new RelayCommand(_ => MainViewModel.HandleClipCompare(this));

            PasteAsPlainTextCommand = new RelayCommand(async _ => await ExecutePasteAs(plainText: true));
            PasteAsRtfCommand = new RelayCommand(async _ => await ExecutePasteAs(plainText: false));
            TransformAndPasteCommand = new RelayCommand(async param => await ExecuteTransformAndPaste(param as string));
            SendToCommand = new RelayCommand(async param => await ExecuteSendTo(param as SendToTarget));
        }

        private async Task<Clip> GetFullClipAsync()
        {
            // This now fetches the full clip but does NOT store it in the viewmodel's state.
            // It's used as a temporary object for operations like paste, open, or tooltip generation.
            var fullClip = await Controller.GetClipByIdAsync(Id);
            return fullClip;
        }

        private async Task ExecuteTransformAndPaste(string? transformType)
        {
            if (transformType == null) return;

            await Controller.MoveClipToTopAsync(Id);
            MainViewModel.RefreshClipList();

            var transformedContent = await Controller.GetTransformedContentAsync(Id, transformType);
            if (!string.IsNullOrEmpty(transformedContent))
            {
                await _pastingService.PasteTextAsync(transformedContent);
                await Controller.UpdatePasteCountAsync();
            }

            var mainWindow = Application.Current.MainWindow;
            if (mainWindow != null)
            {
                mainWindow.Hide();
            }
        }

        private async Task ExecutePasteAs(bool plainText)
        {
            Application.Current.MainWindow?.Hide();

            var clip = await GetFullClipAsync();
            if (clip == null) return;

            await _pastingService.PasteClipAsync(clip, forcePlainText: plainText);
            await Controller.UpdatePasteCountAsync();
        }

        private async Task ExecuteOpen()
        {
            var fullClip = await GetFullClipAsync();
            if (fullClip?.Content == null) return;
            try
            {
                var path = fullClip.Content.Trim();
                if (string.IsNullOrEmpty(path)) return;

                System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(path) { UseShellExecute = true });
            }
            catch (Exception ex)
            {
                Core.Configuration.LogManager.Log(ex, $"Failed to open path: {fullClip.Content}");
                _notificationService.Show("Error", $"Could not open path: {ex.Message}", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        private async Task ExecuteSendTo(SendToTarget? target)
        {
            if (target == null)
            {
                Core.Configuration.LogManager.LogDebug("SENDTO_DIAG: ExecuteSendTo called with null target.");
                return;
            }
            Core.Configuration.LogManager.LogDebug($"SENDTO_DIAG: ExecuteSendTo called for target: {target.Name} ({target.Path})");

            var clip = await GetFullClipAsync();
            if (clip?.Content == null)
            {
                Core.Configuration.LogManager.LogDebug("SENDTO_DIAG: Clip content is null, aborting.");
                return;
            }

            string contentPath;

            if (IsFileBased)
            {
                contentPath = clip.Content.Trim();
            }
            else
            {
                var extension = ClipType switch
                {
                    AppConstants.ClipTypes.CodeSnippet => ".txt",
                    AppConstants.ClipTypes.Rtf => ".rtf",
                    _ => ".txt"
                };
                var tempFilePath = Path.Combine(Path.GetTempPath(), $"cliptoo_sendto_{Guid.NewGuid()}{extension}");
                await File.WriteAllTextAsync(tempFilePath, clip.Content);
                contentPath = tempFilePath;
            }

            try
            {
                string args;
                if (string.IsNullOrWhiteSpace(target.Arguments))
                {
                    args = $"\"{contentPath}\"";
                }
                else
                {
                    args = string.Format(target.Arguments, contentPath);
                }
                Process.Start(new ProcessStartInfo(target.Path, args) { UseShellExecute = true });
            }
            catch (Exception ex)
            {
                Core.Configuration.LogManager.Log(ex, $"Failed to send to path: {target.Path} with content {contentPath}");
                _notificationService.Show("Error", $"Could not send to '{target.Name}'.", ControlAppearance.Danger, SymbolRegular.ErrorCircle24);
            }
        }

        public void UpdateClip(Clip clip, string theme)
        {
            Interlocked.Increment(ref _currentThumbnailLoadId);

            _clip = clip;
            _theme = theme;
            IsPinned = clip.IsPinned;
            ThumbnailSource = null;
            HasThumbnail = false;
            FileProperties = null;
            FileTypeInfo = null;
            PageTitle = null;
            IsSourceMissing = false;

            if (IsFileBased && !string.IsNullOrEmpty(Content))
            {
                if (ClipType == AppConstants.ClipTypes.Folder)
                {
                    IsSourceMissing = !Directory.Exists(Content.Trim());
                }
                else
                {
                    IsSourceMissing = !File.Exists(Content.Trim());
                }
            }

            UpdatePreviewText();
            ClearTooltipContent();
            _ = LoadIconsAsync();
            _ = LoadThumbnailAsync(theme);

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
        }

        private async Task LoadIconsAsync()
        {
            ClipTypeIcon = await _iconProvider.GetIconAsync(ClipType, 20);
        }

        private async Task LoadQuickPasteIconAsync()
        {
            if (Index > 0 && Index <= 9)
            {
                // Generate a high-resolution icon source, the UI will scale it down.
                QuickPasteIcon = await _iconProvider.GetIconAsync(Index.ToString(), 64);
            }
            else
            {
                QuickPasteIcon = null;
            }
        }

        private void UpdatePreviewText()
        {
            string basePreview;
            if (!string.IsNullOrWhiteSpace(_clip.MatchContext))
            {
                basePreview = _clip.MatchContext;
            }
            else if (_clip.ClipType == AppConstants.ClipTypes.Rtf)
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
            Preview = basePreview.ReplaceLineEndings(" ").Trim();
            OnPropertyChanged(nameof(Preview));
        }

        public async Task LoadTooltipContentAsync()
        {
            if (_isTooltipContentLoaded) return;
            DebugUtils.LogMemoryUsage($"LoadTooltipContentAsync START (ID: {Id})");

            var clipForTooltip = await GetFullClipAsync();

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
                    Core.Configuration.LogManager.Log(ex, "Failed to read text file for tooltip preview.");
                    textFileContent = $"Error reading file: {ex.Message}";
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
                // First pass: Count total lines accurately
                int totalLines = 0;
                using (var reader = new StringReader(contentForTooltip))
                {
                    while (reader.ReadLine() != null)
                    {
                        totalLines++;
                    }
                }
                if (totalLines == 0 && contentForTooltip.Length > 0 && !contentForTooltip.Contains('\n'))
                {
                    totalLines = 1;
                }

                // Second pass: Build the formatted string with line numbers
                var sb = new StringBuilder();
                int numberPadding = totalLines.ToString().Length;
                int currentLineNumber = 0;
                using (var reader = new StringReader(contentForTooltip))
                {
                    string? line;
                    while ((line = reader.ReadLine()) != null)
                    {
                        currentLineNumber++;
                        if (currentLineNumber > MaxTooltipLines)
                        {
                            break;
                        }
                        sb.AppendLine($"{(currentLineNumber).ToString().PadLeft(numberPadding)} | {line}");
                    }
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
                    sb.AppendLine($"\n... (truncated - {totalLines - MaxTooltipLines} more lines)");
                }

                TooltipTextContent = sb.ToString();
            }
            else
            {
                TooltipTextContent = null;
                LineCountInfo = null;
            }

            OnPropertyChanged(nameof(TooltipTextContent));
            OnPropertyChanged(nameof(LineCountInfo));
        }


        private async Task TogglePinAsync()
        {
            IsPinned = !IsPinned;
            await Controller.TogglePinAsync(Id, IsPinned);
            MainViewModel.HandleClipPinToggle(this);
        }

        private async Task DeleteAsync()
        {
            await Controller.DeleteClipAsync(_clip);
            MainViewModel.HandleClipDeletion(this);
        }

        public async Task LoadThumbnailAsync(string theme)
        {
            var loadId = _currentThumbnailLoadId;
            string? newThumbnailPath = await _clipDetailsLoader.GetThumbnailAsync(this, _thumbnailService, _webMetadataService, theme);

            if (loadId != _currentThumbnailLoadId)
            {
                return;
            }

            if (!string.IsNullOrEmpty(newThumbnailPath))
            {
                try
                {
                    var bitmapImage = new BitmapImage();
                    bitmapImage.BeginInit();
                    bitmapImage.UriSource = new Uri(newThumbnailPath, UriKind.Absolute);
                    bitmapImage.CacheOption = BitmapCacheOption.OnLoad;
                    bitmapImage.EndInit();
                    bitmapImage.Freeze();
                    ThumbnailSource = bitmapImage;
                    HasThumbnail = true;
                }
                catch (Exception ex)
                {
                    Core.Configuration.LogManager.Log(ex, $"Failed to load thumbnail image source from path: {newThumbnailPath}");
                    ThumbnailSource = null;
                    HasThumbnail = false;
                }
            }
            else
            {
                ThumbnailSource = null;
                HasThumbnail = false;
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
                ImagePreviewPath = null;
                return;
            }

            ImagePreviewPath = await _clipDetailsLoader.GetImagePreviewAsync(this, _thumbnailService, largestDimension, _theme);
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

        public void RaisePasteAsPropertiesChanged()
        {
            OnPropertyChanged(nameof(CanPasteAsPlainText));
            OnPropertyChanged(nameof(CanPasteAsRtf));
        }
    }

}