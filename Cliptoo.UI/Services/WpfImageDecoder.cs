using System.IO;
using System.Windows.Media.Imaging;
using Cliptoo.Core.Services;
using SixLabors.ImageSharp;

namespace Cliptoo.UI.Services
{
    public class WpfImageDecoder : IImageDecoder
    {
        private readonly IImageDecoder _baseDecoder;

        public WpfImageDecoder(ImageSharpDecoder baseDecoder)
        {
            _baseDecoder = baseDecoder;
        }

        public async Task<Image?> DecodeAsync(Stream stream, string extension)
        {
            if (extension == ".ICO")
            {
                return await DecodeIcoAsync(stream);
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
                catch (System.Exception)
                {
                    return null;
                }
            }).ConfigureAwait(false);
        }
    }
}