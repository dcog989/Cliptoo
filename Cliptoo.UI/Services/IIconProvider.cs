using System.Threading.Tasks;
using System.Windows.Media;
using Cliptoo.Core.Services;

namespace Cliptoo.UI.Services
{
    public interface IIconProvider : IIconCacheManager
    {
        Task<ImageSource?> GetIconAsync(string key, int size = 20);
    }
}