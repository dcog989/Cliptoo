using System.Threading.Tasks;

namespace Cliptoo.Core.Database
{
    public interface IDatabaseInitializer
    {
        Task InitializeAsync();
    }
}