using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Database
{
    public interface IDatabaseStatsService
    {
        Task<DbStats> GetStatsAsync();
        Task UpdatePasteCountAsync();
        Task UpdateLastCleanupTimestampAsync();
    }
}