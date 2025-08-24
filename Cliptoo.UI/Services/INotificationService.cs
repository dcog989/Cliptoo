using Wpf.Ui.Controls;

namespace Cliptoo.UI.Services
{
    public interface INotificationService
    {
        void Show(string title, string message, ControlAppearance appearance = ControlAppearance.Primary, SymbolRegular icon = SymbolRegular.Info24, int timeout = 3);
    }
}