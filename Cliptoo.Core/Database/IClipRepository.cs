using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Database
{
    public interface IClipRepository
    {
        Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, string tagSearchPrefix, CancellationToken cancellationToken);
        Task<Clip?> GetClipByIdAsync(int id);
        Task<Clip?> GetPreviewClipByIdAsync(int id);
        Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed);
        Task<int> UpdateClipContentAsync(int id, string content);
        Task DeleteClipAsync(int id);
        Task ToggleFavoriteAsync(int id, bool isFavorite);
        Task UpdateTimestampAsync(int id);
        IAsyncEnumerable<Clip> GetAllFileBasedClipsAsync();
        Task UpdateClipTypesAsync(Dictionary<int, string> updates);
        IAsyncEnumerable<string> GetAllImageClipPathsAsync();
        IAsyncEnumerable<string> GetAllLinkClipUrlsAsync();
        Task IncrementPasteCountAsync(int clipId);
        IAsyncEnumerable<Clip> GetAllClipsAsync(bool favoriteOnly);
        Task<int> AddClipsAsync(IEnumerable<Clip> clips);
        Task UpdateClipTagsAsync(int id, string tags);
    }
}