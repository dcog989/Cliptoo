using Cliptoo.Core.Services;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Services
{
    public interface IClipDetailsLoader
    {
        Task<string?> GetThumbnailAsync(ClipViewModel vm, IThumbnailService thumbnailService, IWebMetadataService webMetadataService, string theme);
        Task<string?> GetImagePreviewAsync(ClipViewModel vm, IThumbnailService thumbnailService, uint size, string theme);
        Task<string?> GetPageTitleAsync(ClipViewModel vm, IWebMetadataService webMetadataService, CancellationToken token);
        Task<(string? properties, string? typeInfo, bool isMissing)> GetFilePropertiesAsync(ClipViewModel vm, CancellationToken token);
    }
}