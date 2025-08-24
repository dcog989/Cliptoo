using Cliptoo.Core;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Native;
using Cliptoo.UI.Helpers;
using System.IO;
using System.Windows;
using System.Windows.Media.Imaging;

namespace Cliptoo.UI.Services
{
    public class PastingService : IPastingService
    {
        private readonly CliptooController _controller;
        private readonly InputSimulator _inputSimulator;

        public PastingService(CliptooController controller, InputSimulator inputSimulator)
        {
            _controller = controller;
            _inputSimulator = inputSimulator;
        }

        public async Task PasteClipAsync(Clip clip, bool? forcePlainText = null)
        {
            await _controller.MoveClipToTopAsync(clip.Id);

            var dataObject = new DataObject();
            var settings = _controller.GetSettings();
            bool pasteAsPlainText = forcePlainText ?? settings.PasteAsPlainText;

            if (pasteAsPlainText)
            {
                string plainText = (clip.ClipType == AppConstants.ClipTypes.Rtf && clip.Content != null)
                    ? RtfUtils.ToPlainText(clip.Content)
                    : clip.Content ?? "";
                dataObject.SetText(plainText, TextDataFormat.UnicodeText);
            }
            else
            {
                switch (clip.ClipType)
                {
                    case AppConstants.ClipTypes.Image:
                        if (File.Exists(clip.Content))
                        {
                            var bitmap = new BitmapImage();
                            bitmap.BeginInit();
                            bitmap.UriSource = new Uri(clip.Content, UriKind.Absolute);
                            bitmap.CacheOption = BitmapCacheOption.OnLoad;
                            bitmap.EndInit();
                            bitmap.Freeze();
                            dataObject.SetImage(bitmap);
                        }
                        break;

                    case AppConstants.ClipTypes.Rtf:
                        dataObject.SetData(DataFormats.Rtf, clip.Content);
                        dataObject.SetText(RtfUtils.ToPlainText(clip.Content ?? string.Empty), TextDataFormat.UnicodeText);
                        break;

                    default:
                        dataObject.SetText(clip.Content, TextDataFormat.UnicodeText);
                        break;
                }
            }

            _controller.ClipboardMonitor.Pause();
            try
            {
                if (await ClipboardUtils.SafeSet(() => Clipboard.SetDataObject(dataObject, true)))
                {
                    _inputSimulator.SendPaste();
                }
            }
            finally
            {
                await Task.Delay(300);
                _controller.ClipboardMonitor.Resume();
            }
        }

        public async Task PasteTextAsync(string text)
        {
            var dataObject = new DataObject();
            dataObject.SetText(text, TextDataFormat.UnicodeText);

            _controller.ClipboardMonitor.Pause();
            try
            {
                if (await ClipboardUtils.SafeSet(() => Clipboard.SetDataObject(dataObject, true)))
                {
                    _inputSimulator.SendPaste();
                }
            }
            finally
            {
                // TODO: A more robust solution would be a short-term hash suppression in the ClipboardMonitor - ignore the next clipboard change if it matches the hash of the content just pasted.
                await Task.Delay(300);
                _controller.ClipboardMonitor.Resume();
            }
        }
    }
}