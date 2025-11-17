using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using Cliptoo.Core;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Services;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using ICSharpCode.AvalonEdit.Highlighting;
using ICSharpCode.AvalonEdit.Highlighting.Xshd;
using System.Xml;
using Wpf.Ui.Appearance;
using Microsoft.Data.Sqlite;
using Wpf.Ui;
using Wpf.Ui.Controls;
using IThemeService = Cliptoo.UI.Services.IThemeService;

namespace Cliptoo.UI.ViewModels
{
    internal class ClipViewerViewModel : ViewModelBase, IDisposable
    {
        private readonly int _clipId;
        private readonly IClipDataService _clipDataService;
        private readonly ISyntaxHighlighter _syntaxHighlighter;
        private readonly IContentDialogService _contentDialogService;
        private readonly IThemeService _themeService;
        private readonly FontFamily _editorFontFamily;
        private readonly double _editorFontSize;
        private string _originalClipType = string.Empty;
        private string _documentContent = string.Empty;
        private string _clipInfo = string.Empty;
        private string _originalContent = string.Empty;
        private string _tags = string.Empty;
        private string _originalTags = string.Empty;
        private Core.Database.Models.Clip? _loadedClip;
        private bool _disposedValue;

        public event EventHandler? OnRequestClose;
        public event EventHandler? OnClipUpdated;

        public int ClipId => _clipId;

        public string DocumentContent
        {
            get => _documentContent;
            set => SetProperty(ref _documentContent, value);
        }

        public string Tags
        {
            get => _tags;
            set => SetProperty(ref _tags, value);
        }

        public string ClipInfo { get => _clipInfo; set => SetProperty(ref _clipInfo, value); }
        public IHighlightingDefinition? SyntaxHighlighting { get; private set; }
        public ISettingsService SettingsService { get; }

        public FontFamily EditorFontFamily => _editorFontFamily;

        public double EditorFontSize => _editorFontSize;

        public ICommand SaveChangesCommand { get; }
        public ICommand CancelCommand { get; }

        public ClipViewerViewModel(int clipId, IClipDataService clipDataService, ISettingsService settingsService, IFontProvider fontProvider, ISyntaxHighlighter syntaxHighlighter, IContentDialogService contentDialogService, IThemeService themeService)
        {
            ArgumentNullException.ThrowIfNull(clipDataService);
            ArgumentNullException.ThrowIfNull(settingsService);
            ArgumentNullException.ThrowIfNull(fontProvider);
            ArgumentNullException.ThrowIfNull(syntaxHighlighter);
            ArgumentNullException.ThrowIfNull(contentDialogService);
            ArgumentNullException.ThrowIfNull(themeService);

            _clipId = clipId;
            _clipDataService = clipDataService;
            SettingsService = settingsService;
            _syntaxHighlighter = syntaxHighlighter;
            _contentDialogService = contentDialogService;
            _themeService = themeService;

            SaveChangesCommand = new RelayCommand(async _ => await ExecuteSaveChanges());
            CancelCommand = new RelayCommand(_ => OnRequestClose?.Invoke(this, EventArgs.Empty));

            var settings = SettingsService.Settings;
            _editorFontSize = settings.PreviewFontSize;
            _editorFontFamily = fontProvider.GetFont(settings.PreviewFontFamily);

            _themeService.ThemeChanged += OnThemeChanged;
            _ = LoadClipAsync();
        }

        private void OnThemeChanged(object? sender, EventArgs e)
        {
            if (_loadedClip is not null)
            {
                LoadSyntaxHighlighting(_loadedClip);
            }
        }

        private async Task LoadClipAsync()
        {
            _loadedClip = await _clipDataService.GetClipByIdAsync(_clipId);
            if (_loadedClip == null)
            {
                OnRequestClose?.Invoke(this, EventArgs.Empty);
                return;
            }

            _originalClipType = _loadedClip.ClipType;

            string? contentForInfo;
            if (_originalClipType == AppConstants.ClipTypeRtf)
            {
                contentForInfo = RtfUtils.ToPlainText(_loadedClip.Content ?? string.Empty);
            }
            else
            {
                contentForInfo = _loadedClip.Content;
            }

            _originalContent = contentForInfo ?? string.Empty;
            DocumentContent = contentForInfo ?? string.Empty;
            _originalTags = _loadedClip.Tags ?? string.Empty;
            Tags = _loadedClip.Tags ?? string.Empty;

            var lineCount = string.IsNullOrEmpty(contentForInfo) ? 0 : contentForInfo.Split('\n').Length;
            var formattedSize = Cliptoo.UI.Helpers.FormatUtils.FormatBytes(_loadedClip.SizeInBytes);
            ClipInfo = $"Size: {formattedSize}    Lines: {lineCount}";

            LoadSyntaxHighlighting(_loadedClip);
        }

        private void LoadSyntaxHighlighting(Core.Database.Models.Clip clip)
        {
            var definitionName = _syntaxHighlighter.GetHighlightingDefinition(clip.ClipType, clip.Content ?? string.Empty);
            if (definitionName == null)
            {
                SyntaxHighlighting = null;
                OnPropertyChanged(nameof(SyntaxHighlighting));
                return;
            }

            var theme = ApplicationThemeManager.GetAppTheme();
            if (theme == ApplicationTheme.Unknown)
            {
                theme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
            }

            IHighlightingDefinition? highlighting;
            if (theme == ApplicationTheme.Dark)
            {
                if (definitionName is "C#" or "XML" or "JavaScript")
                {
                    highlighting = null;
                    var uri = new Uri("pack://application:,,,/Assets/AvalonEditThemes/CSharp-Dark.xshd");
                    var resourceInfo = Application.GetResourceStream(uri);
                    if (resourceInfo != null)
                    {
                        using var stream = resourceInfo.Stream;
                        using var reader = new XmlTextReader(stream);
                        highlighting = HighlightingLoader.Load(reader, HighlightingManager.Instance);
                    }
                    // If dark theme fails to load, fall back to default C# highlighting
                    SyntaxHighlighting = highlighting ?? HighlightingManager.Instance.GetDefinition(definitionName);
                }
                else
                {
                    // Disable highlighting for other languages in dark mode to avoid bad default colors.
                    SyntaxHighlighting = null;
                }
            }
            else
            {
                // Light theme uses default highlighting
                SyntaxHighlighting = HighlightingManager.Instance.GetDefinition(definitionName);
            }
            OnPropertyChanged(nameof(SyntaxHighlighting));
        }

        private async Task ExecuteSaveChanges()
        {
            try
            {
                bool contentChanged = DocumentContent != _originalContent;
                bool tagsChanged = Tags != _originalTags;
                int finalClipId = _clipId;

                if (contentChanged)
                {
                    finalClipId = await _clipDataService.UpdateClipContentAsync(_clipId, DocumentContent);
                }

                if (tagsChanged)
                {
                    await _clipDataService.UpdateClipTagsAsync(finalClipId, Tags);
                }

                if (contentChanged || tagsChanged)
                {
                    OnClipUpdated?.Invoke(this, EventArgs.Empty);
                }
                OnRequestClose?.Invoke(this, EventArgs.Empty);
            }
            catch (SqliteException ex) when (ex.SqliteErrorCode == 19 && ex.Message.Contains("ContentHash", StringComparison.Ordinal)) // UNIQUE constraint failed
            {
                var dialog = new ContentDialog
                {
                    Title = "Save Failed",
                    Content = "A clip with this exact content already exists. Please modify the content to be unique.",
                    CloseButtonText = "OK"
                };
                await _contentDialogService.ShowAsync(dialog, CancellationToken.None);
            }
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _themeService.ThemeChanged -= OnThemeChanged;
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