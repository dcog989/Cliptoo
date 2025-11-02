using System;
using System.Threading.Tasks;

namespace Cliptoo.Core.Services
{
    public interface IWebMetadataService
    {
        Task<string?> GetFaviconAsync(Uri url, string? theme);
        Task<string?> GetPageTitleAsync(Uri url);
        void ClearCache();
        void ClearCacheForUrl(Uri url);
        Task<int> PruneCacheAsync();
    }
}