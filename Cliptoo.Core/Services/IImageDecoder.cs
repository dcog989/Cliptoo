using System.IO;
using System.Threading.Tasks;
using SixLabors.ImageSharp;

namespace Cliptoo.Core.Services
{
    public interface IImageDecoder
    {
        Task<Image?> DecodeAsync(Stream stream, string extension);
    }
}