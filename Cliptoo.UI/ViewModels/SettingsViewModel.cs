using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Media;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using Microsoft.Data.Sqlite;
using Microsoft.Extensions.DependencyInjection;
using Wpf.Ui;

namespace Cliptoo.UI.ViewModels
{
    internal partial class SettingsViewModel : ViewModelBase, IDisposable
    {
        private readonly IDatabaseService _databaseService;
        private readonly ISettingsService _settingsService;
        private readonly IContentDialogService _contentDialogService;
        private readonly IStartupManagerService _startupManagerService;
        private readonly IServiceProvider _serviceProvider;
        private readonly IFontProvider _fontProvider;
        private readonly IIconProvider _iconProvider;
        private readonly Cliptoo.UI.Services.IThemeService _themeService;
        private readonly System.Timers.Timer _saveDebounceTimer;
        private Settings _settings = null!;
        private DbStats _stats = null!;
        private string _selectedFontFamily;
        private string _selectedPreviewFontFamily;
        private double _accentHue;
        private SolidColorBrush _accentBrush = new(Colors.Transparent);
        private Brush _oklchHueBrush;
        private string _currentPage = "General";
        public string AppVersion { get; }
        public string ExePathDir { get; }
        public string ExePathFile { get; }
        public string SettingsFolderPath { get; }
        public string TempDataPath { get; }
        public static Uri GitHubUrl { get; } = new("https://github.com/dcgog989/Cliptoo");
        public ObservableCollection<SendToTarget> SendToTargets => Settings.SendToTargets;

        public SettingsViewModel(IDatabaseService databaseService, ISettingsService settingsService, IContentDialogService contentDialogService, IStartupManagerService startupManagerService, IServiceProvider serviceProvider, IFontProvider fontProvider, IIconProvider iconProvider, Cliptoo.UI.Services.IThemeService themeService)
        {
            _databaseService = databaseService;
            _settingsService = settingsService;
            _contentDialogService = contentDialogService;
            _startupManagerService = startupManagerService;
            _serviceProvider = serviceProvider;
            _fontProvider = fontProvider;
            _iconProvider = iconProvider;
            _themeService = themeService;
            Settings = _settingsService.Settings;
            Settings.PropertyChanged += OnSettingsPropertyChanged;
            _selectedFontFamily = Settings.FontFamily;
            _selectedPreviewFontFamily = Settings.PreviewFontFamily;
            _oklchHueBrush = new SolidColorBrush(Colors.Transparent);
            AppVersion = System.Reflection.Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "0.0.0";

            var exePath = Process.GetCurrentProcess().MainModule?.FileName ?? "Not available";
            ExePathDir = Path.GetDirectoryName(exePath) ?? "Not available";
            ExePathFile = Path.GetFileName(exePath);

            SaveSettingsCommand = new RelayCommand(_ => SyncAndSaveSettings());
            ClearHistoryCommand = new RelayCommand(async _ => await HandleClearHistory(), _ => !IsBusy);
            ClearCachesCommand = new RelayCommand(async _ =>
            {
                if (IsBusy) return;
                IsBusy = true;
                try
                {
                    await Task.Run(() => _databaseService.ClearCaches()).ConfigureAwait(true);
                    await ShowInformationDialogAsync("Caches Cleared", new System.Windows.Controls.TextBlock { Text = "All cached thumbnails and temporary files have been deleted." }).ConfigureAwait(true);
                }
                finally
                {
                    IsBusy = false;
                }
            }, _ => !IsBusy);

            RunHeavyMaintenanceCommand = new RelayCommand(async _ =>
                {
                    if (IsBusy) return;
                    IsBusy = true;
                    try
                    {
                        var result = await Task.Run(async () => await _databaseService.RunHeavyMaintenanceNowAsync().ConfigureAwait(false)).ConfigureAwait(true);
                        await InitializeAsync().ConfigureAwait(true);

                        var results = new List<string>();
                        if (result.DbClipsCleaned > 0) results.Add($"- Removed {result.DbClipsCleaned} old clips.");
                        if (result.ImageCachePruned > 0) results.Add($"- Pruned {result.ImageCachePruned} orphaned image previews.");
                        if (result.FaviconCachePruned > 0) results.Add($"- Pruned {result.FaviconCachePruned} orphaned favicons.");
                        if (result.IconCachePruned > 0) results.Add($"- Pruned {result.IconCachePruned} old icons.");
                        if (result.ReclassifiedClips > 0) results.Add($"- Re-classified {result.ReclassifiedClips} file types.");
                        if (result.TempFilesCleaned > 0) results.Add($"- Cleaned {result.TempFilesCleaned} temporary files.");

                        if (result.DatabaseSizeChangeMb > 0.0)
                        {
                            results.Add($"- Reclaimed {result.DatabaseSizeChangeMb:F2} MB of database space.");
                        }
                        else if (result.DatabaseSizeChangeMb < 0.0)
                        {
                            results.Add($"- Database size increased by {-result.DatabaseSizeChangeMb:F2} MB.");
                        }
                        else
                        {
                            results.Add("- Database compaction ran, but size did not change.");
                        }

                        UIElement dialogContent;
                        if (results.Count > 0)
                        {
                            var stackPanel = new System.Windows.Controls.StackPanel();
                            stackPanel.Children.Add(new System.Windows.Controls.TextBlock { Text = "Maintenance routine completed.", Margin = new Thickness(0, 0, 0, 10) });
                            foreach (var line in results)
                            {
                                stackPanel.Children.Add(new System.Windows.Controls.TextBlock { Text = line });
                            }
                            dialogContent = stackPanel;
                        }
                        else
                        {
                            dialogContent = new System.Windows.Controls.TextBlock { Text = "No items required cleaning." };
                        }

                        await ShowInformationDialogAsync("Maintenance Complete", dialogContent).ConfigureAwait(true);
                    }
                    finally
                    {
                        IsBusy = false;
                    }
                }, _ => !IsBusy);

            RemoveDeadheadClipsCommand = new RelayCommand(async _ => await HandleRemoveDeadheadClips(), _ => !IsBusy);
            ClearOversizedCommand = new RelayCommand(async _ => await HandleClearOversized(), _ => !IsBusy);
            ChangePageCommand = new RelayCommand(p => CurrentPage = p as string ?? "General");
            BrowseCompareToolCommand = new RelayCommand(_ => ExecuteBrowseCompareTool());

            SettingsFolderPath = Path.Combine(App.AppDataRoamingPath, "Cliptoo");
            TempDataPath = Path.Combine(App.AppDataLocalPath, "Cliptoo");

            OpenGitHubUrlCommand = new RelayCommand(_ => Process.Start(new ProcessStartInfo(GitHubUrl.AbsoluteUri) { UseShellExecute = true }));
            OpenSettingsFolderCommand = new RelayCommand(_ => Process.Start(new ProcessStartInfo(SettingsFolderPath) { UseShellExecute = true }));
            OpenTempDataFolderCommand = new RelayCommand(_ => Process.Start(new ProcessStartInfo(TempDataPath) { UseShellExecute = true }));
            OpenAcknowledgementsWindowCommand = new RelayCommand(ShowAcknowledgementsWindow);
            OpenExeFolderCommand = new RelayCommand(_ => Process.Start(new ProcessStartInfo(ExePathDir) { UseShellExecute = true }));
            BrowseCompareToolCommand = new RelayCommand(_ => ExecuteBrowseCompareTool());
            AddSendToTargetCommand = new RelayCommand(_ => ExecuteAddSendToTarget());
            RemoveSendToTargetCommand = new RelayCommand(param => ExecuteRemoveSendToTarget(param as SendToTarget));
            MoveSendToTargetUpCommand = new RelayCommand(ExecuteMoveSendToTargetUp, CanExecuteMoveSendToTargetUp);
            MoveSendToTargetDownCommand = new RelayCommand(ExecuteMoveSendToTargetDown, CanExecuteMoveSendToTargetDown);

            SystemFonts = new ObservableCollection<string>();
            _ = PopulateFontsAsync();

            Settings.SendToTargets.CollectionChanged += OnSendToTargetsCollectionChanged;
            foreach (var target in Settings.SendToTargets)
            {
                target.PropertyChanged += OnSendToTargetPropertyChanged;
            }

            _saveDebounceTimer = new System.Timers.Timer(500);
            _saveDebounceTimer.Elapsed += OnDebounceTimerElapsed;
            _saveDebounceTimer.AutoReset = false;

            UpdateOklchHueBrush();
            InitializeAccentColor();

            if (string.IsNullOrWhiteSpace(Settings.CompareToolPath))
            {
                var (foundPath, _) = _serviceProvider.GetRequiredService<ICompareToolService>().FindCompareTool();
                if (!string.IsNullOrEmpty(foundPath))
                {
                    Settings.CompareToolPath = foundPath;
                    OnPropertyChanged(nameof(CompareToolPath));
                }
            }

            _ = LoadIconsAsync();
        }

        private void OnSettingsPropertyChanged(object? sender, PropertyChangedEventArgs e)
        {
            OnPropertyChanged(e.PropertyName);

            switch (e.PropertyName)
            {
                case nameof(Settings.StartWithWindows):
                    _startupManagerService.SetStartup(Settings.StartWithWindows);
                    LogManager.LogDebug($"Setting 'Start with Windows' changed to: {Settings.StartWithWindows}");
                    break;
                case nameof(Settings.AccentChromaLevel):
                    _themeService.ApplyAccentColor(_accentHue);
                    UpdateOklchHueBrush();
                    AccentBrush = (SolidColorBrush)Application.Current.Resources["AccentBrush"];
                    break;
                case nameof(Settings.Theme):
                    LogManager.LogDebug($"Setting 'Theme' changed to: {Settings.Theme}");
                    _themeService.ApplyThemeFromSettings();
                    UpdateOklchHueBrush();
                    AccentBrush = (SolidColorBrush)Application.Current.Resources["AccentBrush"];
                    break;
                case nameof(Settings.FontFamily):
                    if (_selectedFontFamily != Settings.FontFamily)
                    {
                        _selectedFontFamily = Settings.FontFamily;
                        OnPropertyChanged(nameof(SelectedFontFamily));
                    }
                    break;
                case nameof(Settings.PreviewFontFamily):
                    if (_selectedPreviewFontFamily != Settings.PreviewFontFamily)
                    {
                        _selectedPreviewFontFamily = Settings.PreviewFontFamily;
                        OnPropertyChanged(nameof(SelectedPreviewFontFamily));
                    }
                    break;
            }

            // For most properties, just debounce a save.
            // Hotkeys are handled separately and save immediately.
            if (e.PropertyName != nameof(Settings.Hotkey) &&
                e.PropertyName != nameof(Settings.PreviewHotkey) &&
                e.PropertyName != nameof(Settings.QuickPasteHotkey))
            {
                DebounceSave();
            }
        }

        public string StatsSummary
        {
            get
            {
                if (Stats == null) return "Loading stats...";
                var pinnedText = Stats.PinnedClips > 0 ? $" (+{Stats.PinnedClips:N0} pinned)" : "";
                var totalUnpinned = Stats.TotalClips - Stats.PinnedClips;
                return $"{totalUnpinned:N0} clips{pinnedText} in database using {Stats.DatabaseSizeMb} MB.";
            }
        }

        public async Task InitializeAsync()
        {
            try
            {
                Stats = await _databaseService.GetStatsAsync().ConfigureAwait(true);
                OnPropertyChanged(nameof(StatsSummary));
            }
            catch (SqliteException ex)
            {
                LogManager.LogCritical(ex, "Failed to load database stats.");
                Stats = new DbStats();
            }
        }

        private void OnSendToTargetsCollectionChanged(object? sender, NotifyCollectionChangedEventArgs e)
        {
            if (e.NewItems != null)
            {
                foreach (SendToTarget item in e.NewItems)
                {
                    item.PropertyChanged += OnSendToTargetPropertyChanged;
                }
            }

            if (e.OldItems != null)
            {
                foreach (SendToTarget item in e.OldItems)
                {
                    item.PropertyChanged -= OnSendToTargetPropertyChanged;
                }
            }

            DebounceSave();
        }

        private void OnSendToTargetPropertyChanged(object? sender, PropertyChangedEventArgs e)
        {
            DebounceSave();
        }

        private async Task LoadIconsAsync()
        {
            LogoIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Logo, 138).ConfigureAwait(true);
            TrashIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Trash, 16).ConfigureAwait(true);
        }

        private void OnDebounceTimerElapsed(object? sender, System.Timers.ElapsedEventArgs e)
        {
            SyncAndSaveSettings();
        }

        private void SyncAndSaveSettings()
        {
            _settingsService.SaveSettings();
        }

        private void DebounceSave()
        {
            _saveDebounceTimer.Stop();
            _saveDebounceTimer.Start();
        }

        public void Cleanup()
        {
            Dispose();
        }

        private async Task PopulateFontsAsync()
        {
            IsFontsLoading = true;
            try
            {
                var fonts = await Task.Run(() =>
                {
                    var fontList = new List<string> { "Source Code Pro" };
                    fontList.AddRange(Fonts.SystemFontFamilies.Select(f => f.Source).Distinct().OrderBy(s => s));
                    return fontList;
                }).ConfigureAwait(true);

                foreach (var fontName in fonts)
                {
                    SystemFonts.Add(fontName);
                }

                SelectedFontFamily = SystemFonts.Contains(Settings.FontFamily) ? Settings.FontFamily : SystemFonts.First();
                SelectedPreviewFontFamily = SystemFonts.Contains(Settings.PreviewFontFamily) ? Settings.PreviewFontFamily : SystemFonts.First();
            }
            catch (Exception ex)
            {
                LogManager.LogCritical(ex, "Failed to populate system fonts. Using fallbacks.");
                if (SystemFonts.Count == 0)
                {
                    SystemFonts.Add("Source Code Pro");
                    SystemFonts.Add("Segoe UI");
                    SystemFonts.Add("Arial");
                }
                SelectedFontFamily = SystemFonts.Contains(Settings.FontFamily) ? Settings.FontFamily : SystemFonts.First();
                SelectedPreviewFontFamily = SystemFonts.Contains(Settings.PreviewFontFamily) ? Settings.PreviewFontFamily : SystemFonts.First();
            }
            finally
            {
                IsFontsLoading = false;
            }
        }

        public void Dispose()
        {
            Settings.PropertyChanged -= OnSettingsPropertyChanged;
            _saveDebounceTimer.Elapsed -= OnDebounceTimerElapsed;
            _saveDebounceTimer.Dispose();
            if (Settings?.SendToTargets is not null)
            {
                Settings.SendToTargets.CollectionChanged -= OnSendToTargetsCollectionChanged;
                foreach (var target in Settings.SendToTargets)
                {
                    target.PropertyChanged -= OnSendToTargetPropertyChanged;
                }
            }
            GC.SuppressFinalize(this);
        }
    }

    internal enum ClearHistoryResult { Cancel, ClearUnpinned, ClearAll }

    internal class ClearHistoryDialogViewModel : ViewModelBase
    {
        private bool _deletePinned;
        public bool DeletePinned { get => _deletePinned; set => SetProperty(ref _deletePinned, value); }
        private bool _deleteLogs;
        public bool DeleteLogs { get => _deleteLogs; set => SetProperty(ref _deleteLogs, value); }
        public ClearHistoryResult Result { get; set; } = ClearHistoryResult.Cancel;
    }

}