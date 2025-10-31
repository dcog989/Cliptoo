using System;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Interfaces
{
    public record MaintenanceResult(
        int DbClipsCleaned,
        int ImageCachePruned,
        int FaviconCachePruned,
        int ReclassifiedClips,
        int TempFilesCleaned,
        int IconCachePruned,
        int ClipboardImagesPruned,
        double DatabaseSizeChangeMb
    );

    public interface IDatabaseService
    {
        Task<DbStats> GetStatsAsync();
        Task ClearHistoryAsync();
        Task ClearAllHistoryAsync();
        Task ClearFavoriteClipsAsync();
        void ClearCaches();
        Task<MaintenanceResult> RunHeavyMaintenanceNowAsync();
        Task<int> RemoveDeadheadClipsAsync();
        Task<int> ClearOversizedClipsAsync(uint sizeMb);
        Task<int> ReclassifyAllClipsAsync();
        int CleanupTempFiles();
        Task<string> ExportToJsonStringAsync(bool favoriteOnly);
        Task<int> ImportFromJsonAsync(string jsonContent);
        event EventHandler? HistoryCleared;
    }
}