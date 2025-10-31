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
            bool isFileOperation = !pasteAsPlainText && (clip.ClipType.StartsWith("file_", StringComparison.Ordinal) || clip.ClipType == AppConstants.ClipTypeFolder);

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
                var normalizedForHash = allFilesText.Replace("\r\n", "\n", StringComparison.Ordinal);
                var hash = HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(normalizedForHash));
                _clipboardMonitor.SuppressNextClip(new[] { hash });

                await ClipboardUtils.SafeSet(() => NativeClipboardHelper.SetFileDropList(fileDropList)).ConfigureAwait(false);
            }
        }

        private async Task HandleDataSetAsync(Clip clip, bool pasteAsPlainText)
        {
            var dataObject = new DataObject();
            if (pasteAsPlainText)
            {
                string plainText = (clip.ClipType == AppConstants.ClipTypeRtf && clip.Content != null)
                    ? RtfUtils.ToPlainText(clip.Content)
                    : clip.Content ?? "";
                dataObject.SetText(plainText, TextDataFormat.UnicodeText);
            }
            else
            {
                switch (clip.ClipType)
                {
                    case AppConstants.ClipTypeImage:
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
                    case AppConstants.ClipTypeRtf:
                        dataObject.SetData(DataFormats.Rtf, clip.Content);
                        dataObject.SetText(RtfUtils.ToPlainText(clip.Content ?? string.Empty), TextDataFormat.UnicodeText);
                        break;
                    default:
                        dataObject.SetText(clip.Content, TextDataFormat.UnicodeText);
                        break;
                }
            }

            var hashesToSuppress = new HashSet<ulong>();
            if (!pasteAsPlainText && clip.ClipType == AppConstants.ClipTypeRtf && clip.Content is not null)
            {
                hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(clip.Content)));
            }

            if (dataObject.GetDataPresent(DataFormats.UnicodeText))
            {
                var text = dataObject.GetData(DataFormats.UnicodeText) as string ?? "";
                var normalizedText = text.Replace("\r\n", "\n", StringComparison.Ordinal);
                hashesToSuppress.Add(HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(normalizedText)));
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
                _clipboardMonitor.SuppressNextClip(hashesToSuppress);
            }

            await ClipboardUtils.SafeSet(() => Clipboard.SetDataObject(dataObject, true)).ConfigureAwait(false);
        }

        public async Task PasteTextAsync(string text)
        {
            var normalizedText = text.Replace("\r\n", "\n", StringComparison.Ordinal);
            var hash = HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(normalizedText));
            _clipboardMonitor.SuppressNextClip(new[] { hash });

            LogManager.LogDebug($"Pasting transformed text. Length: {text.Length}.");
            if (await ClipboardUtils.SafeSet(() => Clipboard.SetText(text, TextDataFormat.UnicodeText)).ConfigureAwait(false))
            {
                await InputSimulator.SendPasteAsync().ConfigureAwait(false);
            }
        }

    }
}