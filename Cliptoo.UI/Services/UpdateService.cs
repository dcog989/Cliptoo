using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Velopack;
using Velopack.Sources;

namespace Cliptoo.UI.Services
{
    internal class UpdateService : IUpdateService
    {
        private readonly ISettingsService _settingsService;

        public UpdateService(ISettingsService settingsService)
        {
            _settingsService = settingsService;
        }

        public async Task CheckForUpdatesAsync(CancellationToken cancellationToken)
        {
            if (!_settingsService.Settings.AutoUpdate)
            {
                LogManager.LogInfo("Auto-update is disabled by user setting.");
                return;
            }

            LogManager.LogInfo("Auto-update is enabled. Checking for updates...");
            try
            {
                // TODO: or is it `.../releases`?
                var source = new GithubSource("https://github.com/dcog989/cliptoo", null, false);
                var um = new UpdateManager(source);

                if (um.IsPortable)
                {
                    LogManager.LogInfo("Application is running in portable mode. Skipping update check.");
                    return;
                }

                var updateInfo = await um.CheckForUpdatesAsync().WaitAsync(cancellationToken).ConfigureAwait(false);
                if (cancellationToken.IsCancellationRequested) return;

                if (updateInfo != null)
                {
                    LogManager.LogInfo($"Update found: {updateInfo.TargetFullRelease.Version}. Downloading...");
                    await um.DownloadUpdatesAsync(updateInfo, cancelToken: cancellationToken).ConfigureAwait(false);
                    if (cancellationToken.IsCancellationRequested) return;
                    LogManager.LogInfo("Update downloaded. Applying and restarting...");
                    um.ApplyUpdatesAndRestart(updateInfo);
                }
                else
                {
                    LogManager.LogInfo("No updates found.");
                }
            }
            catch (Exception ex) when (ex is not OperationCanceledException)
            {
                LogManager.LogWarning($"Velopack update check failed: {ex.Message}");
            }
        }
    }
}