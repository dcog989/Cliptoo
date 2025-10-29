namespace Cliptoo.UI.Services
{
    internal interface IUpdateService
    {
        Task CheckForUpdatesAsync(CancellationToken cancellationToken);
    }
}