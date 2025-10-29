using System.ComponentModel;
using System.Windows.Media;

namespace Cliptoo.UI.Services
{
    public interface IUiSharedResources : INotifyPropertyChanged
    {
        ImageSource? LogoIcon { get; }
        ImageSource? MenuIcon { get; }
        ImageSource? WasTrimmedIcon { get; }
        ImageSource? MultiLineIcon { get; }
        ImageSource? PinIcon { get; }
        ImageSource? PinIcon16 { get; }
        ImageSource? ErrorIcon { get; }
        Task InitializeAsync();
    }
}