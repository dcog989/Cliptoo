using Cliptoo.Core;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using System.Windows.Input;
using System.Windows.Media;

namespace Cliptoo.UI.ViewModels
{
    public class ClipViewerViewModel : ViewModelBase
    {
        private readonly int _clipId;
        private readonly CliptooController _controller;
        private readonly FontFamily _editorFontFamily;
        private readonly double _editorFontSize;
        private string _originalClipType = string.Empty;
        private string _documentContent = string.Empty;
        private string _clipInfo = string.Empty;
        private string _originalContent = string.Empty;

        public event Action? OnRequestClose;
        public event Action? OnClipUpdated;

        public int ClipId => _clipId;

        public string DocumentContent
        {
            get => _documentContent;
            set => SetProperty(ref _documentContent, value);
        }

        public string ClipInfo { get => _clipInfo; set => SetProperty(ref _clipInfo, value); }

        public FontFamily EditorFontFamily => _editorFontFamily;

        public double EditorFontSize => _editorFontSize;

        public ICommand SaveChangesCommand { get; }
        public ICommand CancelCommand { get; }
        public CliptooController Controller => _controller;

        public ClipViewerViewModel(int clipId, CliptooController controller, IFontProvider fontProvider)
        {
            _clipId = clipId;
            _controller = controller;

            SaveChangesCommand = new RelayCommand(async _ => await ExecuteSaveChanges());
            CancelCommand = new RelayCommand(_ => OnRequestClose?.Invoke());

            var settings = _controller.GetSettings();
            _editorFontSize = settings.PreviewFontSize;
            _editorFontFamily = fontProvider.GetFont(settings.PreviewFontFamily);

            _ = LoadClipAsync();
        }

        private async Task LoadClipAsync()
        {
            var clip = await _controller.GetClipByIdAsync(_clipId);
            if (clip == null)
            {
                OnRequestClose?.Invoke();
                return;
            }

            _originalClipType = clip.ClipType;

            string? contentForInfo;
            if (_originalClipType == AppConstants.ClipTypes.Rtf)
            {
                contentForInfo = RtfUtils.ToPlainText(clip.Content ?? string.Empty);
            }
            else
            {
                contentForInfo = clip.Content;
            }

            _originalContent = contentForInfo ?? string.Empty;
            DocumentContent = contentForInfo ?? string.Empty;

            var lineCount = string.IsNullOrEmpty(contentForInfo) ? 0 : contentForInfo.Split('\n').Length;
            var formattedSize = FormatUtils.FormatBytes(clip.SizeInBytes);
            ClipInfo = $"Size: {formattedSize}    Lines: {lineCount}";
        }

        private async Task ExecuteSaveChanges()
        {
            if (DocumentContent != _originalContent)
            {
                await _controller.UpdateClipContentAsync(_clipId, DocumentContent);
                OnClipUpdated?.Invoke();
            }
            OnRequestClose?.Invoke();
        }
    }
}