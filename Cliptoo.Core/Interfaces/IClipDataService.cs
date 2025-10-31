using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Interfaces
{
    public interface IClipDataService
    {
        Task<List<Clip>> GetClipsAsync(uint limit = 100, uint offset = 0, string searchTerm = "", string filterType = "all", string tagSearchPrefix = "##", CancellationToken cancellationToken = default);
        Task<Clip?> GetClipByIdAsync(int id);
        Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed);
        Task UpdateClipContentAsync(int id, string newContent);
        Task DeleteClipAsync(Clip clip);
        Task ToggleFavoriteAsync(int id, bool isFavorite);
        Task MoveClipToTopAsync(int id);
        Task IncrementPasteCountAsync(int clipId);
        event EventHandler? NewClipAdded;
        event EventHandler? ClipDeleted;
        Task UpdateClipTagsAsync(int id, string tags);
    }
}