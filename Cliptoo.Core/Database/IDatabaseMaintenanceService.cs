using System.Threading.Tasks;

namespace Cliptoo.Core.Database
{
    public interface IDatabaseMaintenanceService
    {
        Task<int> ClearHistoryAsync();
        Task<int> ClearAllHistoryAsync();
        Task<int> ClearFavoriteClipsAsync();
        Task CompactDbAsync();
        Task<int> PerformCleanupAsync(uint days, uint maxClips, bool forceCompact = false);
        Task<int> RemoveDeadheadClipsAsync();
        Task<int> ClearOversizedClipsAsync(uint sizeMb);
    }
}