namespace Cliptoo.Core.Services
{
    public interface IIconCacheManager
    {
        int CleanupIconCache();
        void ClearCache();
    }
}