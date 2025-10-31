using System.IO;
using System.Windows.Media.Imaging;
using Cliptoo.Core.Services;
using SixLabors.ImageSharp;

namespace Cliptoo.UI.Services
{
    internal class WpfImageDecoder : IImageDecoder
    {
        private readonly IImageDecoder _baseDecoder;

        public WpfImageDecoder(ImageSharpDecoder baseDecoder)
        {
            _baseDecoder = baseDecoder;
        }

        public async Task<Image?> DecodeAsync(Stream stream, string extension)
        {
            ArgumentNullException.ThrowIfNull(stream);

            if (extension == ".ICO")
            {
                var image = await DecodeIcoAsync(stream);

                if (image == null)
                {
                    if (stream.CanSeek)
                    {
                        stream.Position = 0;
                    }
                    // Pass a non-ICO, non-JXL extension to let ImageSharp auto-detect
                    return await _baseDecoder.DecodeAsync(stream, ".PNG");
                }

                return image;
            }

            return await _baseDecoder.DecodeAsync(stream, extension);
        }

        private static async Task<Image?> DecodeIcoAsync(Stream stream)
        {
            return await Task.Run(async () =>
            {
                try
                {
                    var decoder = new IconBitmapDecoder(stream, BitmapCreateOptions.PreservePixelFormat, BitmapCacheOption.OnLoad);
                    var bestFrame = decoder.Frames.OrderByDescending(f => f.Width * f.Height).FirstOrDefault();

                    if (bestFrame == null) return null;

                    using var ms = new MemoryStream();
                    var encoder = new PngBitmapEncoder();
                    encoder.Frames.Add(bestFrame);
                    encoder.Save(ms);
                    ms.Position = 0;
                    return await Image.LoadAsync(ms).ConfigureAwait(false);
                }
                catch (Exception ex) when (ex is NotSupportedException or ArgumentException or FileFormatException)
                {
                    return null;
                }
            }).ConfigureAwait(false);
        }
    }
}