using System.IO;
using System.Windows;
using System.Windows.Threading;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Native;
using Cliptoo.Core.Services;
using Cliptoo.UI.Native;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Velopack;
using Wpf.Ui;
using Wpf.Ui.Tray;

namespace Cliptoo.UI
{
    public partial class App : Application, IDisposable
    {
        private IHost? _host;
        private Mutex? _mutex;
        private bool _disposedValue;

        public static IServiceProvider Services { get; private set; } = null!;
        public static string AppDataRoamingPath { get; private set; } = string.Empty;
        public static string AppDataLocalPath { get; private set; } = string.Empty;

        public App()
        {
            // Logging initialization is now in Main() to ensure it runs after Velopack.
            this.DispatcherUnhandledException += OnDispatcherUnhandledException;
        }

        [STAThread]
        private static void Main(string[] args)
        {
            try
            {
                // It's important to Run() as early as possible in app startup.
                VelopackApp.Build().Run();
            }
            catch (Exception ex)
            {
                MessageBox.Show($"A critical error occurred with the application updater. Please report this issue.\n\nError: {ex.Message}", "Cliptoo Update Error", MessageBoxButton.OK, MessageBoxImage.Error);
                Environment.Exit(1);
            }

            // Use Velopack's built-in portable check. The UpdateManager needs a valid (even if non-functional)
            // URL to be instantiated correctly without crashing.
            var um = new UpdateManager("https://github.com/dcog989/cliptoo");
            bool isPortable = um.IsPortable;

            var exePath = System.AppContext.BaseDirectory;
            if (isPortable)
            {
                AppDataRoamingPath = Path.Combine(exePath, "Data");
                AppDataLocalPath = Path.Combine(exePath, "Data-Local");
                Directory.CreateDirectory(AppDataRoamingPath);
                Directory.CreateDirectory(AppDataLocalPath);
            }
            else
            {
                AppDataRoamingPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
                AppDataLocalPath = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            }

            LogManager.Initialize(AppDataRoamingPath);
            LogManager.LogDebug($"App Main() started. Portable mode: {isPortable}");

            try
            {
                var app = new App();
                app.InitializeComponent();
                app.Run();
            }
            catch (Exception ex)
            {
                LogManager.LogCritical(ex, "A fatal error occurred during application startup after Velopack initialization.");
                var logFolder = Path.Combine(AppDataRoamingPath, "Cliptoo", "Logs");
                MessageBox.Show($"A fatal file access error occurred during application startup. Please check the log files in '{logFolder}'.", "Cliptoo Startup Error", MessageBoxButton.OK, MessageBoxImage.Error);
                Environment.Exit(1);
            }
        }

        protected override async void OnStartup(StartupEventArgs e)
        {
            _mutex = new Mutex(true, "{CAEEC8DB-0AC3-45E2-BDAF-4B5BB2F47531}-Cliptoo", out bool createdNew);

            if (!createdNew)
            {
                WindowUtils.BringExistingInstanceToFront();
                Application.Current.Shutdown();
                return;
            }

            LogManager.LogDebug("App.OnStartup called.");
            try
            {
                _host = Host.CreateDefaultBuilder()
                    .ConfigureServices((context, services) =>
                    {
                        var dbFolder = Path.Combine(AppDataRoamingPath, "Cliptoo", "Database");
                        Directory.CreateDirectory(dbFolder);
                        var dbPath = Path.Combine(dbFolder, "cliptoo_history.db");

                        services.AddSingleton<IDatabaseInitializer>(new DatabaseInitializer(dbPath));
                        services.AddSingleton<IClipRepository>(new ClipRepository(dbPath));
                        services.AddSingleton<IDatabaseMaintenanceService>(new DatabaseMaintenanceService(dbPath));
                        services.AddSingleton<IDatabaseStatsService>(new DatabaseStatsService(dbPath));
                        services.AddSingleton<IDbManager, DbManager>();

                        services.AddSingleton<ISettingsManager>(new SettingsManager(AppDataRoamingPath));
                        services.AddSingleton<ISettingsService, SettingsService>();
                        services.AddSingleton<IClipDataService, ClipDataService>();
                        services.AddSingleton<IClipboardService, ClipboardService>();

                        services.AddSingleton<IDatabaseService>(sp => new DatabaseService(
                           sp.GetRequiredService<IDbManager>(),
                           sp.GetRequiredService<IThumbnailService>(),
                           sp.GetRequiredService<IWebMetadataService>(),
                           sp.GetRequiredService<Core.Services.IIconCacheManager>(),
                           sp.GetRequiredService<IFileTypeClassifier>(),
                           sp.GetRequiredService<ISettingsService>(),
                           AppDataLocalPath
                       ));

                        services.AddSingleton<IAppInteractionService, AppInteractionService>();
                        services.AddSingleton<CliptooController>();

                        services.AddSingleton<IFileTypeClassifier>(new FileTypeClassifier(AppDataRoamingPath));
                        services.AddSingleton<IContentProcessor, ContentProcessor>();
                        services.AddSingleton<ImageSharpDecoder>();
                        services.AddSingleton<IImageDecoder, WpfImageDecoder>();
                        services.AddSingleton<IThumbnailService>(sp => new ThumbnailService(AppDataLocalPath, sp.GetRequiredService<IImageDecoder>()));
                        services.AddSingleton<IWebMetadataService>(sp => new WebMetadataService(AppDataLocalPath, sp.GetRequiredService<IImageDecoder>()));
                        services.AddSingleton<ISyntaxHighlighter, SyntaxHighlighter>();
                        services.AddSingleton<IClipboardMonitor, ClipboardMonitor>();
                        services.AddSingleton<ITextTransformer, TextTransformer>();
                        services.AddSingleton<ICompareToolService, CompareToolService>();
                        services.AddSingleton<IconProvider>(sp => new IconProvider(sp.GetRequiredService<ISettingsService>(), AppDataLocalPath));
                        services.AddSingleton<IIconProvider>(sp => sp.GetRequiredService<IconProvider>());
                        services.AddSingleton<Core.Services.IIconCacheManager>(sp => sp.GetRequiredService<IconProvider>());

                        services.AddSingleton<ISnackbarService, SnackbarService>();
                        services.AddSingleton<INotificationService, NotificationService>();
                        services.AddSingleton<IFontProvider, FontProvider>();
                        services.AddSingleton<IClipViewModelFactory, ClipViewModelFactory>();
                        services.AddSingleton<IClipDetailsLoader, ClipDetailsLoader>();
                        services.AddSingleton<IPastingService, PastingService>();
                        services.AddSingleton<IWindowPositioner, WindowPositioner>();
                        services.AddSingleton<IStartupManagerService, StartupManagerService>();
                        services.AddSingleton<Cliptoo.UI.Services.IThemeService, Cliptoo.UI.Services.ThemeService>();
                        services.AddSingleton<INotifyIconService, CustomNotifyIconService>();
                        services.AddSingleton<IContentDialogService, ContentDialogService>();
                        services.AddHostedService<ApplicationHostService>();

                        services.AddSingleton<MainViewModel>(sp => new MainViewModel(
                            sp.GetRequiredService<IClipDataService>(),
                            sp.GetRequiredService<IClipboardService>(),
                            sp.GetRequiredService<ISettingsService>(),
                            sp.GetRequiredService<IDatabaseService>(),
                            sp.GetRequiredService<IAppInteractionService>(),
                            sp,
                            sp.GetRequiredService<IClipViewModelFactory>(),
                            sp.GetRequiredService<IPastingService>(),
                            sp.GetRequiredService<IFontProvider>(),
                            sp.GetRequiredService<INotificationService>(),
                            sp.GetRequiredService<IIconProvider>()
                        ));
                        services.AddTransient<SettingsViewModel>(sp => new SettingsViewModel(
                            sp.GetRequiredService<IDatabaseService>(),
                            sp.GetRequiredService<ISettingsService>(),
                            sp.GetRequiredService<IContentDialogService>(),
                            sp.GetRequiredService<IStartupManagerService>(),
                            sp,
                            sp.GetRequiredService<IFontProvider>(),
                            sp.GetRequiredService<IIconProvider>(),
                            sp.GetRequiredService<Cliptoo.UI.Services.IThemeService>()
                        ));

                        services.AddSingleton<MainWindow>();
                        services.AddTransient<SettingsWindow>();
                        services.AddTransient<ClearHistoryDialog>();
                        services.AddTransient<ClearOversizedDialog>();
                        services.AddTransient<ClipViewerWindow>();
                        services.AddTransient<AcknowledgementsWindow>();
                    }).Build();

                Services = _host.Services;
                LogManager.LogDebug("Host built and services configured.");

                await _host.StartAsync().ConfigureAwait(false);
                LogManager.LogDebug("Host started.");
            }
            catch (Exception ex)
            {
                LogManager.LogCritical(ex, "A fatal error occurred during application startup.");
                MessageBox.Show($"A fatal error occurred during application startup and has been logged. The application will now exit.\n\nError: {ex.Message}", "Cliptoo Startup Error", MessageBoxButton.OK, MessageBoxImage.Error);
                Application.Current.Shutdown();
            }
        }

        protected override async void OnExit(ExitEventArgs e)
        {
            LogManager.LogDebug("App.OnExit called.");
            if (_host != null)
            {
                await _host.StopAsync().ConfigureAwait(false);
            }
            LogManager.LogInfo("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
            LogManager.LogInfo($"Application shutdown complete on {DateTime.Now:yyyyMMdd}.");
            LogManager.LogInfo("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~\r\n");
            LogManager.Shutdown();
            Dispose();
        }

        private void OnDispatcherUnhandledException(object sender, DispatcherUnhandledExceptionEventArgs e)
        {
            try
            {
                LogManager.LogCritical(e.Exception, "An unhandled UI exception occurred, which caused the application to crash.");
            }
            // This catch is intentionally broad. Its purpose is to handle a catastrophic failure
            // where the logging system itself throws an exception. In this last-resort scenario,
            // we must inform the user directly, as logging is no longer an option.
            catch (Exception)
            {
                MessageBox.Show($"A fatal UI error occurred, and the logging system also failed.\n\nOriginal Error:\n{e.Exception.Message}", "Cliptoo Critical Error", MessageBoxButton.OK, MessageBoxImage.Error);
                Application.Current.Shutdown();
                return;
            }

            e.Handled = true;
            var logPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "Cliptoo");
            var message = LogManager.IsInitialized
                ? $"A fatal UI error occurred and has been logged. Please check the logs in '{logPath}'.\n\nError: {e.Exception.Message}"
                : $"A fatal UI error occurred, but the logging service could not be started. Please check folder permissions for '{logPath}'.\n\nError: {e.Exception.Message}";

            MessageBox.Show(message, "Cliptoo Error", MessageBoxButton.OK, MessageBoxImage.Error);
            Application.Current.Shutdown();
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _mutex?.ReleaseMutex();
                    _mutex?.Dispose();
                }

                _disposedValue = true;
            }
        }

        public void Dispose()
        {
            Dispose(disposing: true);
            GC.SuppressFinalize(this);
        }

    }
}