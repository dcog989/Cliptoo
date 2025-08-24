using Wpf.Ui.Tray;

namespace Cliptoo.UI.Services
{
    public class CustomNotifyIconService : NotifyIconService
    {
        public event Action? LeftClicked;
        public event Action? DoubleClicked;

        protected override void OnLeftClick()
        { LeftClicked?.Invoke(); }

        protected override void OnLeftDoubleClick()
        {
            base.OnLeftDoubleClick();
            DoubleClicked?.Invoke();
        }
    }
}