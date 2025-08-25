using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Database
{
    public class DbManager : IDbManager
    {
        private readonly IDatabaseInitializer _initializer;
        private readonly IClipRepository _clipRepository;
        private readonly IDatabaseMaintenanceService _maintenanceService;
        private readonly IDatabaseStatsService _statsService;

        public DbManager(
            IDatabaseInitializer initializer,
            IClipRepository clipRepository,
            IDatabaseMaintenanceService maintenanceService,
            IDatabaseStatsService statsService)
        {
            _initializer = initializer;
            _clipRepository = clipRepository;
            _maintenanceService = maintenanceService;
            _statsService = statsService;
        }

        public Task InitializeAsync() => _initializer.InitializeAsync();
        public Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, CancellationToken cancellationToken = default) => _clipRepository.GetClipsAsync(limit, offset, searchTerm, filterType, cancellationToken);
        public Task<Clip> GetClipByIdAsync(int id) => _clipRepository.GetClipByIdAsync(id);
        public Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed) => _clipRepository.AddClipAsync(content, clipType, sourceApp, wasTrimmed);
        public Task UpdateClipContentAsync(int id, string content) => _clipRepository.UpdateClipContentAsync(id, content);
        public Task DeleteClipAsync(int id) => _clipRepository.DeleteClipAsync(id);
        public Task TogglePinAsync(int id, bool isPinned) => _clipRepository.TogglePinAsync(id, isPinned);
        public Task UpdateTimestampAsync(int id) => _clipRepository.UpdateTimestampAsync(id);
        public Task UpdatePasteCountAsync() => _statsService.UpdatePasteCountAsync();
        public Task<int> ClearHistoryAsync() => _maintenanceService.ClearHistoryAsync();
        public Task<int> ClearAllHistoryAsync() => _maintenanceService.ClearAllHistoryAsync();
        public Task CompactDbAsync() => _maintenanceService.CompactDbAsync();
        public Task<int> PerformCleanupAsync(uint days, uint maxClips, bool forceCompact = false) => _maintenanceService.PerformCleanupAsync(days, maxClips, forceCompact);
        public Task<DbStats> GetStatsAsync() => _statsService.GetStatsAsync();
        public Task<int> RemoveDeadheadClipsAsync() => _maintenanceService.RemoveDeadheadClipsAsync();
        public Task<int> ClearOversizedClipsAsync(uint sizeMb) => _maintenanceService.ClearOversizedClipsAsync(sizeMb);
        public Task<List<Clip>> GetAllFileBasedClipsAsync() => _clipRepository.GetAllFileBasedClipsAsync();
        public Task UpdateClipTypesAsync(Dictionary<int, string> updates) => _clipRepository.UpdateClipTypesAsync(updates);
        public Task<Clip> GetClipPreviewContentByIdAsync(int id) => _clipRepository.GetClipPreviewContentByIdAsync(id);
        public IAsyncEnumerable<string> GetAllImageClipPathsAsync() => _clipRepository.GetAllImageClipPathsAsync();
        public IAsyncEnumerable<string> GetAllLinkClipUrlsAsync() => _clipRepository.GetAllLinkClipUrlsAsync();
        public Task UpdateLastCleanupTimestampAsync() => _statsService.UpdateLastCleanupTimestampAsync();
        public void Dispose() { }
    }
}