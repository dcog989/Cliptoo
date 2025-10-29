namespace Cliptoo.UI.Services
{
    internal interface ITrayManagerService
    {
        void Initialize();
        void OnTaskbarCreated();
        event EventHandler<bool>? ToggleVisibilityRequested;
    }
}