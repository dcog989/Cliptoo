using System.Collections.Generic;
using System.Threading.Tasks;

namespace Cliptoo.Core.Services
{
    public interface IThumbnailService
    {
        Task<string?> GetThumbnailAsync(string imagePath, string? theme);
        Task<string?> GetImagePreviewAsync(string imagePath, uint largestDimension, string? theme);
        Task<int> PruneCacheAsync(IAsyncEnumerable<string> validImagePaths, uint previewSize);
        void ClearCache();
    }
}