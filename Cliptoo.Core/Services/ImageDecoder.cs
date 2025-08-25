using System;
using System.IO;
using System.Threading.Tasks;
using JxlNet;
using SixLabors.ImageSharp;
using SixLabors.ImageSharp.PixelFormats;

namespace Cliptoo.Core.Services
{
    public static class ImageDecoder
    {
        public static async Task<Image?> DecodeAsync(string imagePath)
        {
            var extension = Path.GetExtension(imagePath).ToLowerInvariant();
            await using var stream = File.OpenRead(imagePath);
            return await DecodeAsync(stream, extension);
        }

        public static async Task<Image?> DecodeAsync(Stream stream, string extension)
        {
            // JXL is special because JxlNet takes a byte array
            if (extension == ".jxl")
            {
                await using var ms = new MemoryStream();
                await stream.CopyToAsync(ms);
                return await Task.Run(() => DecodeJxl(ms.ToArray()));
            }

            // Default to ImageSharp for everything else
            return await Image.LoadAsync(stream);
        }


        private static unsafe Image? DecodeJxl(byte[] jxlBytes)
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
            catch (Exception)
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