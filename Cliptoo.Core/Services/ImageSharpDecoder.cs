using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using JxlNet;
using SixLabors.ImageSharp;
using SixLabors.ImageSharp.PixelFormats;

namespace Cliptoo.Core.Services
{
    public class ImageSharpDecoder : IImageDecoder
    {
        public async Task<Image?> DecodeAsync(Stream stream, string extension)
        {
            ArgumentNullException.ThrowIfNull(stream);

            if (extension == ".JXL")
            {
                using var ms = new MemoryStream();
                await stream.CopyToAsync(ms).ConfigureAwait(false);
                return await Task.Run(() => DecodeJxl(ms.ToArray())).ConfigureAwait(false);
            }

            try
            {
                return await Image.LoadAsync(stream).ConfigureAwait(false);
            }
            catch (UnknownImageFormatException)
            {
                // This is expected for formats ImageSharp doesn't support, like ICO.
                // The decorator implementation will handle it.
                return null;
            }
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