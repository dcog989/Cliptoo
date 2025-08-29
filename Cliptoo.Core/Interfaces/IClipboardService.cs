using System.Threading.Tasks;

namespace Cliptoo.Core.Interfaces
{
    public interface IClipboardService
    {
        Task UpdatePasteCountAsync();
        string TransformText(string content, string transformType);
        Task<(bool success, string message)> CompareClipsAsync(int leftClipId, int rightClipId);
        bool IsCompareToolAvailable();
    }
}