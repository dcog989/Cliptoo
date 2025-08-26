using System;
using System.IO;
using System.Threading.Tasks;
using System.Linq;
using System.Runtime.InteropServices;
using System.Windows.Media.Imaging;
using JxlNet;
using SixLabors.ImageSharp;
using SixLabors.ImageSharp.PixelFormats;

namespace Cliptoo.Core.Services
{
    public static class ImageDecoder
    {
        public static async Task<Image?> DecodeAsync(string imagePath)
        {
            var extension = Path.GetExtension(imagePath).ToUpperInvariant();
            FileStream stream = File.OpenRead(imagePath);
            try
            {
                return await DecodeAsync(stream, extension).ConfigureAwait(false);
            }
            finally
            {
                await stream.DisposeAsync().ConfigureAwait(false);
            }
        }

        public static async Task<Image?> DecodeAsync(Stream stream, string extension)
        {
            ArgumentNullException.ThrowIfNull(stream);

            if (extension == ".ICO")
            {
                return await Task.Run(async () =>
                {
                    try
                    {
                        Stream processStream = stream;
                        MemoryStream? createdMemoryStream = null;

                        if (!stream.CanSeek)
                        {
                            createdMemoryStream = new MemoryStream();
                            await stream.CopyToAsync(createdMemoryStream).ConfigureAwait(false);
                            createdMemoryStream.Position = 0;
                            processStream = createdMemoryStream;
                        }
                        else
                        {
                            stream.Position = 0;
                        }

                        try
                        {
                            var decoder = new IconBitmapDecoder(processStream, BitmapCreateOptions.PreservePixelFormat, BitmapCacheOption.OnLoad);
                            var bestFrame = decoder.Frames.OrderByDescending(f => f.Width * f.Height).FirstOrDefault();

                            if (bestFrame == null) return null;

                            using var ms = new MemoryStream();
                            var encoder = new PngBitmapEncoder();
                            encoder.Frames.Add(bestFrame);
                            encoder.Save(ms);
                            ms.Position = 0;
                            return await Image.LoadAsync(ms).ConfigureAwait(false);
                        }
                        finally
                        {
                            if (createdMemoryStream is not null)
                            {
                                await createdMemoryStream.DisposeAsync().ConfigureAwait(false);
                            }
                        }
                    }
                    catch (Exception ex) when (ex is FileFormatException or NotSupportedException)
                    {
                        return null;
                    }
                }).ConfigureAwait(false);
            }

            // JXL is special because JxlNet takes a byte array
            if (extension == ".JXL")
            {
                using var ms = new MemoryStream();
                await stream.CopyToAsync(ms).ConfigureAwait(false);
                return await Task.Run(() => DecodeJxl(ms.ToArray())).ConfigureAwait(false);
            }

            // Default to ImageSharp for everything else
            return await Image.LoadAsync(stream).ConfigureAwait(false);
        }


        private static unsafe Image<Rgba32>? DecodeJxl(byte[] jxlBytes)
        {
            var decoder = Jxl.JxlDecoderCreate(null);
            if (decoder == null) return null;

            try
            {
                fixed (byte* input = jxlBytes)
                {
                    Jxl.JxlDecoderSetInput(decoder, input, (nuint)jxlBytes.Length);
                    Jxl.JxlDecoderCloseInput(decoder);
                    Jxl.JxlDecoderSubscribeEvents(decoder, (int)(JxlDecoderStatus.JXL_DEC_BASIC_INFO | JxlDecoderStatus.JXL_DEC_FULL_IMAGE));
                    var status = Jxl.JxlDecoderProcessInput(decoder);
                    if (status != JxlDecoderStatus.JXL_DEC_BASIC_INFO) return null;
                    var info = new JxlBasicInfo();
                    Jxl.JxlDecoderGetBasicInfo(decoder, &info);

                    int width = (int)info.xsize;
                    int height = (int)info.ysize;
                    byte[] buffer = new byte[width * height * 4];

                    fixed (byte* output = buffer)
                    {
                        var pixelFormat = new JxlPixelFormat
                        {
                            data_type = JxlDataType.JXL_TYPE_UINT8,
                            endianness = JxlEndianness.JXL_NATIVE_ENDIAN,
                            num_channels = 4,
                            align = 0
                        };
                        Jxl.JxlDecoderSetImageOutBuffer(decoder, &pixelFormat, output, (nuint)(buffer.Length * sizeof(byte)));
                        status = Jxl.JxlDecoderProcessInput(decoder);
                        if (status != JxlDecoderStatus.JXL_DEC_FULL_IMAGE) return null;

                        return Image.LoadPixelData<Rgba32>(buffer, width, height);
                    }
                }
            }
            catch (ExternalException)
            {
                return null;
            }
            finally
            {
                if (decoder != null) Jxl.JxlDecoderDestroy(decoder);
            }
        }
    }
}