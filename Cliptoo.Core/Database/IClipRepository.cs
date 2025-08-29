using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.Core.Database
{
    public interface IClipRepository
    {
        Task<List<Clip>> GetClipsAsync(uint limit, uint offset, string searchTerm, string filterType, CancellationToken cancellationToken);
        Task<Clip?> GetClipByIdAsync(int id);
        Task<int> AddClipAsync(string content, string clipType, string? sourceApp, bool wasTrimmed);
        Task UpdateClipContentAsync(int id, string content);
        Task DeleteClipAsync(int id);
        Task TogglePinAsync(int id, bool isPinned);
        Task UpdateTimestampAsync(int id);
        IAsyncEnumerable<Clip> GetAllFileBasedClipsAsync();
        Task UpdateClipTypesAsync(Dictionary<int, string> updates);
        Task<Clip?> GetClipPreviewContentByIdAsync(int id);
        IAsyncEnumerable<string> GetAllImageClipPathsAsync();
        IAsyncEnumerable<string> GetAllLinkClipUrlsAsync();
    }
}