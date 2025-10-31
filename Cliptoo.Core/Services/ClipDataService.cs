using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Services
{
    public class ClipDataService : IClipDataService
    {
        private readonly IDbManager _dbManager;
        private readonly IWebMetadataService _webMetadataService;

        public event EventHandler<ClipAddedEventArgs>? NewClipAdded;
        public event EventHandler? ClipDeleted;

        public ClipDataService(IDbManager dbManager, IWebMetadataService webMetadataService)
        {
            _dbManager = dbManager;
            _webMetadataService = webMetadataService;
        }

        public Task<List<Clip>> GetClipsAsync(uint limit = 100, uint offset = 0, string searchTerm = "", string filterType = "all", string tagSearchPrefix = "##", CancellationToken cancellationToken = default)
        {
            return _dbManager.GetClipsAsync(limit, offset, searchTerm, filterType, tagSearchPrefix, cancellationToken);
        }

        public Task<Clip?> GetClipByIdAsync(int id)
        {
            LogManager.LogDebug($"CLIP_DATA_DIAG: Fetching full clip from DB. ID: {id}.");
            return _dbManager.GetClipByIdAsync(id);
        }

        public async Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed)
        {
            var clipId = await _dbManager.AddClipAsync(content, clipType, sourceApp, wasTrimmed).ConfigureAwait(false);

            // Get the preview version of the newly added clip to broadcast to the UI
            var newClip = await _dbManager.GetPreviewClipByIdAsync(clipId).ConfigureAwait(false);
            if (newClip != null)
            {
                NewClipAdded?.Invoke(this, new ClipAddedEventArgs(newClip));
            }
            else
            {
                // This should realistically never happen if the add succeeded.
                LogManager.LogWarning($"Could not retrieve newly added clip with ID {clipId} to update the UI.");
            }

            return clipId;
        }

        public Task UpdateClipContentAsync(int id, string newContent)
        {
            return _dbManager.UpdateClipContentAsync(id, newContent);
        }

        public async Task DeleteClipAsync(Clip clip)
        {
            ArgumentNullException.ThrowIfNull(clip);

            await _dbManager.DeleteClipAsync(clip.Id).ConfigureAwait(false);
            ClipDeleted?.Invoke(this, EventArgs.Empty);

            if (clip.ClipType == AppConstants.ClipTypes.Link && clip.Content is not null && Uri.TryCreate(clip.Content, UriKind.Absolute, out var uri))
            {
                _webMetadataService.ClearCacheForUrl(uri);
            }
        }

        public Task ToggleFavoriteAsync(int id, bool isFavorite)
        {
            return _dbManager.ToggleFavoriteAsync(id, isFavorite);
        }

        public Task MoveClipToTopAsync(int id)
        {
            return _dbManager.UpdateTimestampAsync(id);
        }

        public Task IncrementPasteCountAsync(int clipId)
        {
            return _dbManager.IncrementPasteCountAsync(clipId);
        }

        public Task UpdateClipTagsAsync(int id, string tags)
        {
            return _dbManager.UpdateClipTagsAsync(id, tags);
        }
    }
}