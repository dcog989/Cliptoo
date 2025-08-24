using Cliptoo.Core.Database.Models;
using System.Threading.Tasks;

namespace Cliptoo.UI.Services
{
    public interface IPastingService
    {
        Task PasteClipAsync(Clip clip, bool? forcePlainText = null);
        Task PasteTextAsync(string text);
    }
}