namespace Cliptoo.UI.Services
{
    public interface IStartupManagerService
    {
        void SetStartup(bool isEnabled);
        bool IsStartupEnabled();
    }
}