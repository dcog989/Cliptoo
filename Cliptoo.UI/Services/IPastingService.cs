using Cliptoo.Core.Database.Models;

namespace Cliptoo.UI.Services
{
    public interface IPastingService
    {
        Task PasteClipAsync(Clip clip, bool? forcePlainText = null);
        Task PasteTextAsync(string text);
    }
}