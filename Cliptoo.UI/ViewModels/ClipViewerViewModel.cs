using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using Cliptoo.Core;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using ICSharpCode.AvalonEdit.Highlighting;
using ICSharpCode.AvalonEdit.Highlighting.Xshd;
using System.Xml;
using Wpf.Ui.Appearance;
using Microsoft.Data.Sqlite;
using Wpf.Ui;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    public class ClipViewerViewModel : ViewModelBase
    {
        private readonly int _clipId;
        private readonly IClipDataService _clipDataService;
        private readonly ISyntaxHighlighter _syntaxHighlighter;
        private readonly IContentDialogService _contentDialogService;
        private readonly FontFamily _editorFontFamily;
        private readonly double _editorFontSize;
        private string _originalClipType = string.Empty;
        private string _documentContent = string.Empty;
        private string _clipInfo = string.Empty;
        private string _originalContent = string.Empty;
        private string _tags = string.Empty;
        private string _originalTags = string.Empty;

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
        public bool IsDarkMode
        {
            get
            {
                var theme = ApplicationThemeManager.GetAppTheme();
                if (theme == ApplicationTheme.Unknown)
                {
                    theme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
                }
                return theme == ApplicationTheme.Dark;
            }
        }

        public ICommand SaveChangesCommand { get; }
        public ICommand CancelCommand { get; }

        public ClipViewerViewModel(int clipId, IClipDataService clipDataService, ISettingsService settingsService, IFontProvider fontProvider, ISyntaxHighlighter syntaxHighlighter, IContentDialogService contentDialogService)
        {
            ArgumentNullException.ThrowIfNull(clipDataService);
            ArgumentNullException.ThrowIfNull(settingsService);
            ArgumentNullException.ThrowIfNull(fontProvider);
            ArgumentNullException.ThrowIfNull(syntaxHighlighter);
            ArgumentNullException.ThrowIfNull(contentDialogService);

            _clipId = clipId;
            _clipDataService = clipDataService;
            SettingsService = settingsService;
            _syntaxHighlighter = syntaxHighlighter;
            _contentDialogService = contentDialogService;

            SaveChangesCommand = new RelayCommand(async _ => await ExecuteSaveChanges());
            CancelCommand = new RelayCommand(_ => OnRequestClose?.Invoke(this, EventArgs.Empty));

            var settings = SettingsService.Settings;
            _editorFontSize = settings.PreviewFontSize;
            _editorFontFamily = fontProvider.GetFont(settings.PreviewFontFamily);

            _ = LoadClipAsync();
        }

        private async Task LoadClipAsync()
        {
            var clip = await _clipDataService.GetClipByIdAsync(_clipId);
            if (clip == null)
            {
                OnRequestClose?.Invoke(this, EventArgs.Empty);
                return;
            }

            _originalClipType = clip.ClipType;

            string? contentForInfo;
            if (_originalClipType == AppConstants.ClipTypeRtf)
            {
                contentForInfo = RtfUtils.ToPlainText(clip.Content ?? string.Empty);
            }
            else
            {
                contentForInfo = clip.Content;
            }

            _originalContent = contentForInfo ?? string.Empty;
            DocumentContent = contentForInfo ?? string.Empty;
            _originalTags = clip.Tags ?? string.Empty;
            Tags = clip.Tags ?? string.Empty;

            var lineCount = string.IsNullOrEmpty(contentForInfo) ? 0 : contentForInfo.Split('\n').Length;
            var formattedSize = FormatUtils.FormatBytes(clip.SizeInBytes);
            ClipInfo = $"Size: {formattedSize}    Lines: {lineCount}";

            LoadSyntaxHighlighting(clip);
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

                if (contentChanged)
                {
                    await _clipDataService.UpdateClipContentAsync(_clipId, DocumentContent);
                }

                if (tagsChanged)
                {
                    await _clipDataService.UpdateClipTagsAsync(_clipId, Tags);
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
    }
}