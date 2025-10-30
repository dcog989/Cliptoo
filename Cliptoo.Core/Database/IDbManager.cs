using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Database
{
    public interface IDbManager : IDisposable
    {
        Task InitializeAsync();
        Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, CancellationToken cancellationToken = default);
        Task<Clip?> GetClipByIdAsync(int id);
        Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed);
        Task UpdateClipContentAsync(int id, string content);
        Task DeleteClipAsync(int id);
        Task ToggleFavoriteAsync(int id, bool isFavorite);
        Task UpdateTimestampAsync(int id);
        Task UpdatePasteCountAsync();
        Task<int> ClearHistoryAsync();
        Task<int> ClearAllHistoryAsync();
        Task<int> ClearFavoriteClipsAsync();
        Task CompactDbAsync();
        Task<int> PerformCleanupAsync(uint days, uint maxClips, bool forceCompact = false);
        Task<DbStats> GetStatsAsync();
        Task<int> RemoveDeadheadClipsAsync();
        Task<int> ClearOversizedClipsAsync(uint sizeMb);
        IAsyncEnumerable<Clip> GetAllFileBasedClipsAsync(); Task UpdateClipTypesAsync(Dictionary<int, string> updates);
        IAsyncEnumerable<string> GetAllImageClipPathsAsync();
        IAsyncEnumerable<string> GetAllLinkClipUrlsAsync();
        Task UpdateLastCleanupTimestampAsync();
        Task IncrementPasteCountAsync(int clipId);
        IAsyncEnumerable<Clip> GetAllClipsAsync(bool favoriteOnly);
        Task<int> AddClipsAsync(IEnumerable<Clip> clips);
    }
}