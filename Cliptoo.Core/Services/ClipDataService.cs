using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;

namespace Cliptoo.Core.Services
{
    public class ClipDataService : IClipDataService
    {
        private readonly IDbManager _dbManager;
        private readonly IWebMetadataService _webMetadataService;
        private readonly LruCache<int, Clip> _clipCache;
        private const int ClipCacheSize = 20;

        public event EventHandler? NewClipAdded;
        public event EventHandler? ClipDeleted;

        public ClipDataService(IDbManager dbManager, IWebMetadataService webMetadataService)
        {
            _dbManager = dbManager;
            _webMetadataService = webMetadataService;
            _clipCache = new LruCache<int, Clip>(ClipCacheSize);
        }

        public Task<List<Clip>> GetClipsAsync(uint limit = 100, uint offset = 0, string searchTerm = "", string filterType = "all", CancellationToken cancellationToken = default)
        {
            return _dbManager.GetClipsAsync(limit, offset, searchTerm, filterType, cancellationToken);
        }

        public async Task<Clip?> GetClipByIdAsync(int id)
        {
            if (_clipCache.TryGetValue(id, out var cachedClip) && cachedClip is not null)
            {
                return cachedClip;
            }
            LogManager.LogDebug($"CLIP_CACHE_DIAG: Miss for Clip ID {id}. Querying database.");

            var clip = await _dbManager.GetClipByIdAsync(id).ConfigureAwait(false);

            if (clip is not null && clip.SizeInBytes < 100 * 1024)
            {
                _clipCache.Add(id, clip);
            }

            return clip;
        }

        public async Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed)
        {
            var clipId = await _dbManager.AddClipAsync(content, clipType, sourceApp, wasTrimmed).ConfigureAwait(false);
            NewClipAdded?.Invoke(this, EventArgs.Empty);
            return clipId;
        }

        public async Task UpdateClipContentAsync(int id, string newContent)
        {
            await _dbManager.UpdateClipContentAsync(id, newContent).ConfigureAwait(false);
            _clipCache.Remove(id);
        }

        public async Task DeleteClipAsync(Clip clip)
        {
            ArgumentNullException.ThrowIfNull(clip);

            await _dbManager.DeleteClipAsync(clip.Id).ConfigureAwait(false);
            _clipCache.Remove(clip.Id);
            ClipDeleted?.Invoke(this, EventArgs.Empty);

            if (clip.ClipType == AppConstants.ClipTypes.Link && clip.Content is not null && Uri.TryCreate(clip.Content, UriKind.Absolute, out var uri))
            {
                _webMetadataService.ClearCacheForUrl(uri);
            }
        }

        public Task TogglePinAsync(int id, bool isPinned)
        {
            return _dbManager.TogglePinAsync(id, isPinned);
        }

        public async Task MoveClipToTopAsync(int id)
        {
            await _dbManager.UpdateTimestampAsync(id).ConfigureAwait(false);
            _clipCache.Remove(id);
        }

        public void ClearCache() => _clipCache.Clear();
    }
}