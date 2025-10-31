using Wpf.Ui.Tray;

namespace Cliptoo.UI.Services
{
    internal class CustomNotifyIconService : NotifyIconService
    {
        public event EventHandler? LeftClicked;
        public event EventHandler? DoubleClicked;

        protected override void OnLeftClick()
        { LeftClicked?.Invoke(this, EventArgs.Empty); }

        protected override void OnLeftDoubleClick()
        {
            base.OnLeftDoubleClick();
            DoubleClicked?.Invoke(this, EventArgs.Empty);
        }
    }
}