using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Media.Imaging;
using Cliptoo.Core;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Native;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;

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
                    if (dataObject.GetDataPresent(DataFormats.UnicodeText))
                    {
                        var text = dataObject.GetData(DataFormats.UnicodeText) as string ?? "";
                        _controller.SuppressNextClip(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(text)));
                    }
                    else if (dataObject.GetDataPresent(DataFormats.Bitmap))
                    {
                        var bitmapSource = dataObject.GetData(DataFormats.Bitmap) as BitmapSource;
                        if (bitmapSource != null)
                        {
                            using var stream = new MemoryStream();
                            var encoder = new PngBitmapEncoder();
                            encoder.Frames.Add(BitmapFrame.Create(bitmapSource));
                            encoder.Save(stream);
                            _controller.SuppressNextClip(HashingUtils.ComputeHash(stream.ToArray()));
                        }
                    }

                    _inputSimulator.SendPaste();
                }
            }
            finally
            {
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
                    var hash = HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(text));
                    _controller.SuppressNextClip(hash);
                    _inputSimulator.SendPaste();
                }
            }
            finally
            {
                _controller.ClipboardMonitor.Resume();
            }
        }
    }
}