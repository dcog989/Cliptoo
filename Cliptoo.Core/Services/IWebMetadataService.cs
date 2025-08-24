using System.Collections.Generic;
using System.Threading.Tasks;

namespace Cliptoo.Core.Services
{
    public interface IWebMetadataService
    {
        Task<string?> GetFaviconAsync(string url);
        Task<string?> GetPageTitleAsync(string url);
        void ClearCache();
        void ClearCacheForUrl(string url);
        Task<int> PruneCacheAsync(IAsyncEnumerable<string> validUrls);
    }
}