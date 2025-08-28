using System.IO;
using System.Windows;
using System.Windows.Threading;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database;
using Cliptoo.Core.Native;
using Cliptoo.Core.Services;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Wpf.Ui;
using Wpf.Ui.Tray;

namespace Cliptoo.UI
{
    public partial class App : Application
    {
        private IHost? _host;

        public static IServiceProvider Services { get; private set; } = null!;

        public App()
        {
            var roamingPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            LogManager.Initialize(roamingPath);
            LogManager.Log("=====================================");
            LogManager.Log("App constructor started.");

            try
            {
                InitializeComponent();
                LogManager.Log("InitializeComponent() completed successfully.");
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, "A fatal error occurred during application startup (InitializeComponent).");
                var logFolder = Path.Combine(roamingPath, "Cliptoo", "Logs");
                MessageBox.Show($"A fatal error occurred during application startup. Please check the log files in '{logFolder}'.", "Cliptoo Startup Error", MessageBoxButton.OK, MessageBoxImage.Error);
                Environment.Exit(1);
            }

            this.DispatcherUnhandledException += OnDispatcherUnhandledException;
        }

        protected override async void OnStartup(StartupEventArgs e)
        {
            LogManager.Log("App.OnStartup called.");
            _host = Host.CreateDefaultBuilder()
                .ConfigureServices((context, services) =>
                {
                    var appDataRoamingPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
                    var appDataLocalPath = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);

                    var dbFolder = Path.Combine(appDataRoamingPath, "Cliptoo", "Database");
                    Directory.CreateDirectory(dbFolder);
                    var dbPath = Path.Combine(dbFolder, "cliptoo_history.db");

                    services.AddSingleton<IDatabaseInitializer>(new DatabaseInitializer(dbPath));
                    services.AddSingleton<IClipRepository>(new ClipRepository(dbPath));
                    services.AddSingleton<IDatabaseMaintenanceService>(new DatabaseMaintenanceService(dbPath));
                    services.AddSingleton<IDatabaseStatsService>(new DatabaseStatsService(dbPath));
                    services.AddSingleton<IDbManager, DbManager>();

                    services.AddSingleton<CliptooController>();
                    services.AddSingleton<ISettingsManager>(new SettingsManager(appDataRoamingPath));
                    services.AddSingleton<IFileTypeClassifier>(new FileTypeClassifier(appDataRoamingPath));
                    services.AddSingleton<IContentProcessor, ContentProcessor>();
                    services.AddSingleton<IThumbnailService>(new ThumbnailService(appDataLocalPath));
                    services.AddSingleton<IWebMetadataService>(new WebMetadataService(appDataLocalPath));
                    services.AddSingleton<ISyntaxHighlighter, SyntaxHighlighter>();
                    services.AddSingleton<IClipboardMonitor, ClipboardMonitor>();
                    services.AddSingleton<ITextTransformer, TextTransformer>();
                    services.AddSingleton<ICompareToolService, CompareToolService>();
                    services.AddSingleton<Core.Services.IIconProvider>(sp => new IconProvider(sp.GetRequiredService<ISettingsManager>(), appDataLocalPath));

                    services.AddSingleton<ISnackbarService, SnackbarService>();
                    services.AddSingleton<INotificationService, NotificationService>();
                    services.AddSingleton<IFontProvider, FontProvider>();
                    services.AddSingleton<IClipViewModelFactory, ClipViewModelFactory>();
                    services.AddSingleton<IClipDetailsLoader, ClipDetailsLoader>();
                    services.AddSingleton<IPastingService, PastingService>();
                    services.AddSingleton<IWindowPositioner, WindowPositioner>();
                    services.AddSingleton<IStartupManagerService, StartupManagerService>();
                    services.AddSingleton<INotifyIconService, CustomNotifyIconService>();
                    services.AddSingleton<IContentDialogService, ContentDialogService>();
                    services.AddHostedService<ApplicationHostService>();

                    services.AddSingleton<MainViewModel>(sp => new MainViewModel(
                        sp.GetRequiredService<CliptooController>(),
                        sp,
                        sp.GetRequiredService<IClipViewModelFactory>(),
                        sp.GetRequiredService<IPastingService>(),
                        sp.GetRequiredService<IFontProvider>(),
                        sp.GetRequiredService<INotificationService>(),
                        sp.GetRequiredService<Core.Services.IIconProvider>()
                    ));
                    services.AddTransient<SettingsViewModel>(sp => new SettingsViewModel(
                        sp.GetRequiredService<CliptooController>(),
                        sp.GetRequiredService<IContentDialogService>(),
                        sp.GetRequiredService<IStartupManagerService>(),
                        sp,
                        sp.GetRequiredService<IFontProvider>(),
                        sp.GetRequiredService<Core.Services.IIconProvider>()
                    ));

                    services.AddSingleton<MainWindow>();
                    services.AddTransient<SettingsWindow>();
                    services.AddTransient<ClearHistoryDialog>();
                    services.AddTransient<ClearOversizedDialog>();
                    services.AddTransient<ClipViewerWindow>();
                    services.AddTransient<AcknowledgementsWindow>();
                }).Build();

            Services = _host.Services;
            LogManager.Log("Host built and services configured.");

            await _host.StartAsync();
            LogManager.Log("Host started.");
        }

        protected override async void OnExit(ExitEventArgs e)
        {
            LogManager.Log("App.OnExit called.");
            if (_host != null)
            {
                await _host.StopAsync();
            }
            LogManager.Log($"Application shutdown complete. ({DateTime.Now:yyyyMMdd})");
            LogManager.Log("=====================================\r\n");
        }

        private void OnDispatcherUnhandledException(object sender, DispatcherUnhandledExceptionEventArgs e)
        {
            try
            {
                LogManager.Log(e.Exception, "An unhandled UI exception occurred, which caused the application to crash.");
            }
            catch (Exception logEx)
            {
                MessageBox.Show($"A fatal UI error occurred, and the logging system also failed.\n\nOriginal Error:\n{e.Exception.Message}\n\nLogging Error:\n{logEx.Message}", "Cliptoo Critical Error", MessageBoxButton.OK, MessageBoxImage.Error);
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
    }
}