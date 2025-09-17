using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Media.Imaging;
using Cliptoo.Core;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Native;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;

namespace Cliptoo.UI.Services
{
    internal class PastingService : IPastingService
    {
        private readonly ISettingsService _settingsService;
        private readonly IClipDataService _clipDataService;
        private readonly IClipboardMonitor _clipboardMonitor;

        public PastingService(ISettingsService settingsService, IClipDataService clipDataService, IClipboardMonitor clipboardMonitor)
        {
            _settingsService = settingsService;
            _clipDataService = clipDataService;
            _clipboardMonitor = clipboardMonitor;
        }

        public async Task PasteClipAsync(Clip clip, bool? forcePlainText = null)
        {
            await SetClipboardContentAsync(clip, forcePlainText).ConfigureAwait(false);
            await InputSimulator.SendPasteAsync().ConfigureAwait(false);
        }

        public async Task SetClipboardContentAsync(Clip clip, bool? forcePlainText = null)
        {
            ArgumentNullException.ThrowIfNull(clip);

            var settings = _settingsService.Settings;
            bool pasteAsPlainText = forcePlainText ?? settings.PasteAsPlainText;
            LogManager.LogDebug($"Setting clipboard content: ID={clip.Id}, AsPlainText={pasteAsPlainText}.");
            bool isFileOperation = !pasteAsPlainText && (clip.ClipType.StartsWith("file_", StringComparison.Ordinal) || clip.ClipType == AppConstants.ClipTypes.Folder);

            if (isFileOperation)
            {
                await HandleFileSetAsync(clip).ConfigureAwait(false);
            }
            else
            {
                await HandleDataSetAsync(clip, pasteAsPlainText).ConfigureAwait(false);
            }
        }

        private async Task HandleFileSetAsync(Clip clip)
        {
            if (clip.Content == null) return;

            var paths = clip.Content.Split(new[] { Environment.NewLine }, StringSplitOptions.RemoveEmptyEntries);
            if (paths.Length > 0)
            {
                var fileDropList = new System.Collections.Specialized.StringCollection();
                fileDropList.AddRange(paths);

                var allFilesText = string.Join(Environment.NewLine, paths);
                var hash1 = HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(allFilesText));
                var hash2 = HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(allFilesText.Replace("\r\n", "\n", StringComparison.Ordinal)));
                _clipboardMonitor.SuppressNextClip(new[] { hash1, hash2 });

                await ClipboardUtils.SafeSet(() => NativeClipboardHelper.SetFileDropList(fileDropList)).ConfigureAwait(false);
            }
        }

        private async Task HandleDataSetAsync(Clip clip, bool pasteAsPlainText)
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

            var hashesToSuppress = new HashSet<ulong>();
            if (!pasteAsPlainText && clip.ClipType == AppConstants.ClipTypes.Rtf && clip.Content is not null)
            {
                hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(clip.Content)));
            }

            if (dataObject.GetDataPresent(DataFormats.UnicodeText))
            {
                var text = dataObject.GetData(DataFormats.UnicodeText) as string ?? "";
                hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(text)));
                var textWithLf = text.Replace("\r\n", "\n", StringComparison.Ordinal);
                hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(textWithLf)));
                var textWithCrLf = textWithLf.Replace("\n", "\r\n", StringComparison.Ordinal);
                hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(textWithCrLf)));
            }
            else if (dataObject.GetDataPresent(DataFormats.Bitmap))
            {
                if (dataObject.GetData(DataFormats.Bitmap) is BitmapSource bitmapSource)
                {
                    using var stream = new MemoryStream();
                    var encoder = new PngBitmapEncoder();
                    encoder.Frames.Add(BitmapFrame.Create(bitmapSource));
                    encoder.Save(stream);
                    hashesToSuppress.Add(HashingUtils.ComputeHash(stream.ToArray()));
                }
            }

            if (hashesToSuppress.Count > 0)
            {
                _clipboardMonitor.SuppressNextClip(hashesToSuppress.ToArray());
            }

            await ClipboardUtils.SafeSet(() => Clipboard.SetDataObject(dataObject, true)).ConfigureAwait(false);
        }

        public async Task PasteTextAsync(string text)
        {
            var hashesToSuppress = new HashSet<ulong>();
            hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(text)));
            var textWithLf = text.Replace("\r\n", "\n", StringComparison.Ordinal);
            hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(textWithLf)));
            var textWithCrLf = textWithLf.Replace("\n", "\r\n", StringComparison.Ordinal);
            hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(textWithCrLf)));

            _clipboardMonitor.SuppressNextClip(hashesToSuppress.ToArray());

            LogManager.LogDebug($"Pasting transformed text. Length: {text.Length}.");
            if (await ClipboardUtils.SafeSet(() => Clipboard.SetText(text, TextDataFormat.UnicodeText)).ConfigureAwait(false))
            {
                await InputSimulator.SendPasteAsync().ConfigureAwait(false);
            }
        }
    }
}