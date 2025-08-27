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
    internal class PastingService : IPastingService
    {
        private readonly CliptooController _controller;

        public PastingService(CliptooController controller)
        {
            _controller = controller;
        }

        public async Task PasteClipAsync(Clip clip, bool? forcePlainText = null)
        {
            ArgumentNullException.ThrowIfNull(clip);

            await _controller.MoveClipToTopAsync(clip.Id).ConfigureAwait(false);

            var settings = _controller.GetSettings();
            bool pasteAsPlainText = forcePlainText ?? settings.PasteAsPlainText;
            bool isFileOperation = !pasteAsPlainText && (clip.ClipType.StartsWith("file_", StringComparison.Ordinal) || clip.ClipType == AppConstants.ClipTypes.Folder);

            _controller.ClipboardMonitor.Pause();
            try
            {
                if (isFileOperation)
                {
                    await HandleFilePasteAsync(clip).ConfigureAwait(false);
                }
                else
                {
                    await HandleDataObjectPasteAsync(clip, pasteAsPlainText).ConfigureAwait(false);
                }
            }
            finally
            {
                _controller.ClipboardMonitor.ResumeMonitoring();
            }
        }

        private Task HandleFilePasteAsync(Clip clip)
        {
            if (clip.Content == null) return Task.CompletedTask;

            var paths = clip.Content.Split(new[] { Environment.NewLine }, StringSplitOptions.RemoveEmptyEntries);
            if (paths.Length > 0)
            {
                var fileDropList = new System.Collections.Specialized.StringCollection();
                fileDropList.AddRange(paths);

                if (NativeClipboardHelper.SetFileDropList(fileDropList))
                {
                    var allFilesText = string.Join(Environment.NewLine, paths);
                    _controller.SuppressNextClip(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(allFilesText)));
                    InputSimulator.SendPaste();
                }
            }
            return Task.CompletedTask;
        }

        private async Task HandleDataObjectPasteAsync(Clip clip, bool pasteAsPlainText)
        {
            var dataObject = new DataObject();
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

            if (await ClipboardUtils.SafeSet(() => Clipboard.SetDataObject(dataObject, true)).ConfigureAwait(false))
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
                InputSimulator.SendPaste();
            }
        }

        public async Task PasteTextAsync(string text)
        {
            var dataObject = new DataObject();
            dataObject.SetText(text, TextDataFormat.UnicodeText);

            _controller.ClipboardMonitor.Pause();
            try
            {
                if (await ClipboardUtils.SafeSet(() => Clipboard.SetDataObject(dataObject, true)).ConfigureAwait(false))
                {
                    var hash = HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(text));
                    _controller.SuppressNextClip(hash);
                    InputSimulator.SendPaste();
                }
            }
            finally
            {
                _controller.ClipboardMonitor.ResumeMonitoring();
            }
        }
    }
}