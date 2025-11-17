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
            _dbManager = dbManager ?? throw new ArgumentNullException(nameof(dbManager));
            _webMetadataService = webMetadataService ?? throw new ArgumentNullException(nameof(webMetadataService));
        }

        public Task<List<Clip>> GetClipsAsync(
            uint limit = 100,
            uint offset = 0,
            string searchTerm = "",
            string filterType = "all",
            string tagSearchPrefix = "##",
            CancellationToken cancellationToken = default)
        {
            return _dbManager.GetClipsAsync(limit, offset, searchTerm, filterType, tagSearchPrefix, cancellationToken);
        }

        public Task<Clip?> GetClipByIdAsync(int id)
        {
            LogManager.LogDebug($"CLIP_DATA_DIAG: Fetching full clip from DB. ID: {id}.");
            return _dbManager.GetClipByIdAsync(id);
        }

        public Task<Clip?> GetPreviewClipByIdAsync(int id)
        {
            return _dbManager.GetPreviewClipByIdAsync(id);
        }

        public async Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed)
        {
            if (string.IsNullOrEmpty(content))
            {
                throw new ArgumentException("Content cannot be null or empty.", nameof(content));
            }

            if (string.IsNullOrEmpty(clipType))
            {
                throw new ArgumentException("Clip type cannot be null or empty.", nameof(clipType));
            }

            try
            {
                var clipId = await _dbManager.AddClipAsync(content, clipType, sourceApp, wasTrimmed).ConfigureAwait(false);

                // Get the preview version of the newly added clip to broadcast to the UI
                var newClip = await _dbManager.GetPreviewClipByIdAsync(clipId).ConfigureAwait(false);
                if (newClip != null)
                {
                    OnNewClipAdded(new ClipAddedEventArgs(newClip));
                }
                else
                {
                    // This should realistically never happen if the add succeeded.
                    LogManager.LogWarning($"Could not retrieve newly added clip with ID {clipId} to update the UI.");
                }

                return clipId;
            }
            catch (Exception ex)
            {
                LogManager.LogCritical(ex, $"Failed to add clip of type '{clipType}'.");
                throw;
            }
        }

        public Task<int> UpdateClipContentAsync(int id, string newContent)
        {
            if (string.IsNullOrEmpty(newContent))
            {
                throw new ArgumentException("New content cannot be null or empty.", nameof(newContent));
            }

            return _dbManager.UpdateClipContentAsync(id, newContent);
        }

        public async Task DeleteClipAsync(Clip clip)
        {
            ArgumentNullException.ThrowIfNull(clip);

            try
            {
                // Cache the values we need before deletion
                var clipId = clip.Id;
                var clipType = clip.ClipType;
                var clipContent = clip.Content;

                await _dbManager.DeleteClipAsync(clipId).ConfigureAwait(false);

                OnClipDeleted();

                // Clear web metadata cache if applicable
                if (clipType == AppConstants.ClipTypeLink &&
                    clipContent is not null &&
                    Uri.TryCreate(clipContent, UriKind.Absolute, out var uri))
                {
                    _webMetadataService.ClearCacheForUrl(uri);
                }
            }
            catch (Exception ex)
            {
                LogManager.LogCritical(ex, $"Failed to delete clip with ID {clip.Id}.");
                throw;
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
            ArgumentNullException.ThrowIfNull(tags);

            return _dbManager.UpdateClipTagsAsync(id, tags);
        }

        /// <summary>
        /// Safely raises the NewClipAdded event.
        /// </summary>
        private void OnNewClipAdded(ClipAddedEventArgs args)
        {
            try
            {
                NewClipAdded?.Invoke(this, args);
            }
#pragma warning disable CA1031 // Do not catch general exception types
            catch (Exception ex)
            {
                // Catch all exceptions from event handlers to prevent one bad subscriber from crashing the application
                LogManager.LogCritical(ex, "Error occurred while invoking NewClipAdded event.");
            }
#pragma warning restore CA1031 // Do not catch general exception types
        }

        /// <summary>
        /// Safely raises the ClipDeleted event.
        /// </summary>
        private void OnClipDeleted()
        {
            try
            {
                ClipDeleted?.Invoke(this, EventArgs.Empty);
            }
#pragma warning disable CA1031 // Do not catch general exception types
            catch (Exception ex)
            {
                // Catch all exceptions from event handlers to prevent one bad subscriber from crashing the application
                LogManager.LogCritical(ex, "Error occurred while invoking ClipDeleted event.");
            }
#pragma warning restore CA1031 // Do not catch general exception types
        }
    }
}
