namespace Cliptoo.UI.Services
{
    internal interface IPlatformService : IDisposable
    {
        void Initialize(IntPtr windowHandle);
        void OnClipboardUpdate();
        void OnHotkeyPressed();
        event EventHandler? HotkeyPressed;
    }
}