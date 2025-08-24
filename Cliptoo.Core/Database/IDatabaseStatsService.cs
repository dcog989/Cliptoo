using Cliptoo.Core.Database.Models;
using System.Threading.Tasks;

namespace Cliptoo.Core.Database
{
    public interface IDatabaseStatsService
    {
        Task<DbStats> GetStatsAsync();
        Task UpdatePasteCountAsync();
        Task UpdateLastCleanupTimestampAsync();
    }
}