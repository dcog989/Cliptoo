using Wpf.Ui;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Services
{
    public class NotificationService : INotificationService
    {
        private readonly ISnackbarService _snackbarService;

        public NotificationService(ISnackbarService snackbarService)
        {
            _snackbarService = snackbarService;
        }

        public void Show(string title, string message, ControlAppearance appearance = ControlAppearance.Primary, SymbolRegular icon = SymbolRegular.Info24, int timeout = 3)
        {
            _snackbarService.Show(title, message, appearance, new SymbolIcon(icon), TimeSpan.FromSeconds(timeout));
        }
    }
}