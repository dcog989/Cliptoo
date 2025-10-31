using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Database
{
    public sealed class DbManager : IDbManager
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
        public Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, string tagSearchPrefix = "##", CancellationToken cancellationToken = default) => _clipRepository.GetClipsAsync(limit, offset, searchTerm, filterType, tagSearchPrefix, cancellationToken);
        public Task<Clip?> GetClipByIdAsync(int id) => _clipRepository.GetClipByIdAsync(id);
        public Task<Clip?> GetPreviewClipByIdAsync(int id) => _clipRepository.GetPreviewClipByIdAsync(id);
        public Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed) => _clipRepository.AddClipAsync(content, clipType, sourceApp, wasTrimmed);
        public Task UpdateClipContentAsync(int id, string content) => _clipRepository.UpdateClipContentAsync(id, content);
        public Task DeleteClipAsync(int id) => _clipRepository.DeleteClipAsync(id);
        public Task ToggleFavoriteAsync(int id, bool isFavorite) => _clipRepository.ToggleFavoriteAsync(id, isFavorite);
        public Task UpdateTimestampAsync(int id) => _clipRepository.UpdateTimestampAsync(id);
        public Task UpdatePasteCountAsync() => _statsService.UpdatePasteCountAsync();
        public Task<int> ClearHistoryAsync() => _maintenanceService.ClearHistoryAsync();
        public Task<int> ClearAllHistoryAsync() => _maintenanceService.ClearAllHistoryAsync();
        public Task<int> ClearFavoriteClipsAsync() => _maintenanceService.ClearFavoriteClipsAsync();
        public Task CompactDbAsync() => _maintenanceService.CompactDbAsync();
        public Task<int> PerformCleanupAsync(uint days, uint maxClips, bool forceCompact = false) => _maintenanceService.PerformCleanupAsync(days, maxClips, forceCompact);
        public Task<DbStats> GetStatsAsync() => _statsService.GetStatsAsync();
        public Task<int> RemoveDeadheadClipsAsync() => _maintenanceService.RemoveDeadheadClipsAsync();
        public Task<int> ClearOversizedClipsAsync(uint sizeMb) => _maintenanceService.ClearOversizedClipsAsync(sizeMb);
        public IAsyncEnumerable<Clip> GetAllFileBasedClipsAsync() => _clipRepository.GetAllFileBasedClipsAsync();
        public Task UpdateClipTypesAsync(Dictionary<int, string> updates) => _clipRepository.UpdateClipTypesAsync(updates);
        public IAsyncEnumerable<string> GetAllImageClipPathsAsync() => _clipRepository.GetAllImageClipPathsAsync();
        public IAsyncEnumerable<string> GetAllLinkClipUrlsAsync() => _clipRepository.GetAllLinkClipUrlsAsync();
        public Task UpdateLastCleanupTimestampAsync() => _statsService.UpdateLastCleanupTimestampAsync();
        public Task IncrementPasteCountAsync(int clipId) => _clipRepository.IncrementPasteCountAsync(clipId);
        public IAsyncEnumerable<Clip> GetAllClipsAsync(bool favoriteOnly) => _clipRepository.GetAllClipsAsync(favoriteOnly);
        public Task<int> AddClipsAsync(IEnumerable<Clip> clips) => _clipRepository.AddClipsAsync(clips);
        public Task UpdateClipTagsAsync(int id, string tags) => _clipRepository.UpdateClipTagsAsync(id, tags);
        public void Dispose()
        {
            // This class doesn't own any disposable resources directly.
            // The DI container manages the lifetime of the injected services.
            GC.SuppressFinalize(this);
        }
    }
}