namespace Cliptoo.UI.Services
{
    public interface IProcessService
    {
        void OpenUrl(Uri url);
        void OpenFolder(string path);
    }
}