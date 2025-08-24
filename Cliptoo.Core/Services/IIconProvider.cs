using System.Threading.Tasks;
using System.Windows.Media;

namespace Cliptoo.Core.Services
{
    public interface IIconProvider
    {
        Task<ImageSource?> GetIconAsync(string key, int size = 20);
        int CleanupIconCache();
    }
}