using System;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Interfaces
{
    public interface IDatabaseService
    {
        Task<DbStats> GetStatsAsync();
        Task ClearHistoryAsync();
        Task ClearAllHistoryAsync();
        void ClearCaches();
        Task<MaintenanceResult> RunHeavyMaintenanceNowAsync();
        Task<int> RemoveDeadheadClipsAsync();
        Task<int> ClearOversizedClipsAsync(uint sizeMb);
        event EventHandler? CachesCleared;
    }
}