using System.Diagnostics;

namespace Cliptoo.UI.Services
{
    internal class ProcessService : IProcessService
    {
        public void OpenFolder(string path)
        {
            Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
        }

        public void OpenUrl(Uri url)
        {
            Process.Start(new ProcessStartInfo(url.AbsoluteUri) { UseShellExecute = true });
        }
    }
}