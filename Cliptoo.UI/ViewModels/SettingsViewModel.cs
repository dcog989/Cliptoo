using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Services;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels.Base;
using Cliptoo.UI.Views;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Win32;
using Wpf.Ui;
using Wpf.Ui.Appearance;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    public class SettingsViewModel : ViewModelBase
    {
        private const double OKLCH_LIGHTNESS = 0.63;
        private const double OKLCH_CHROMA_BRIGHT = 0.22;
        private const double OKLCH_CHROMA_MUTED = 0.10;

        private readonly CliptooController _controller;
        private readonly IContentDialogService _contentDialogService;
        private readonly IStartupManagerService _startupManagerService;
        private readonly IServiceProvider _serviceProvider;
        private readonly IFontProvider _fontProvider;
        private readonly IIconProvider _iconProvider;
        private readonly System.Timers.Timer _saveDebounceTimer;
        private Settings _settings = null!;
        private bool _isCapturingHotkey;
        public bool IsCapturingHotkey { get => _isCapturingHotkey; set => SetProperty(ref _isCapturingHotkey, value); }
        private string? _capturingHotkeyTarget;
        public string? CapturingHotkeyTarget { get => _capturingHotkeyTarget; set => SetProperty(ref _capturingHotkeyTarget, value); }
        private DbStats _stats = null!;
        private string _selectedFontFamily;
        private string _selectedPreviewFontFamily;
        private double _accentHue;
        private SolidColorBrush _accentBrush = new(Colors.Transparent);
        private Brush _oklchHueBrush;
        private string _currentPage = "General";
        private bool _isBusy;
        public bool IsBusy { get => _isBusy; set => SetProperty(ref _isBusy, value); }
        private ImageSource? _logoIcon;
        public ImageSource? LogoIcon { get => _logoIcon; private set => SetProperty(ref _logoIcon, value); }
        public ObservableCollection<SendToTarget> SendToTargets { get; }
        public Settings Settings { get => _settings; set => SetProperty(ref _settings, value); }
        public DbStats Stats { get => _stats; set => SetProperty(ref _stats, value); }
        public SolidColorBrush AccentBrush { get => _accentBrush; set => SetProperty(ref _accentBrush, value); }
        public Brush OklchHueBrush { get => _oklchHueBrush; private set => SetProperty(ref _oklchHueBrush, value); }
        public string CurrentPage { get => _currentPage; set => SetProperty(ref _currentPage, value); }
        public string AppVersion { get; }
        public string ExePathDir { get; }
        public string ExePathFile { get; }
        private bool _isFontsLoading = true;
        public bool IsFontsLoading { get => _isFontsLoading; set => SetProperty(ref _isFontsLoading, value); }

        public string CompareToolPath
        {
            get => Settings.CompareToolPath;
            set
            {
                if (Settings.CompareToolPath != value)
                {
                    Settings.CompareToolPath = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public string Hotkey
        {
            get => Settings.Hotkey;
            set
            {
                if (Settings.Hotkey != value)
                {
                    Settings.Hotkey = value;
                    OnPropertyChanged();
                    _controller.SaveSettings(Settings);
                }
            }
        }

        public string SettingsFolderPath { get; }
        public string TempDataPath { get; }
        public string GitHubUrl => "https://github.com/dcgog989/Cliptoo";

        public ObservableCollection<string> SystemFonts { get; }

        public string SelectedFontFamily
        {
            get => _selectedFontFamily;
            set
            {
                if (SetProperty(ref _selectedFontFamily, value) && value != null)
                {
                    Settings.FontFamily = value;
                    DebounceSave();
                }
            }
        }

        public string SelectedPreviewFontFamily
        {
            get => _selectedPreviewFontFamily;
            set
            {
                if (SetProperty(ref _selectedPreviewFontFamily, value) && value != null)
                {
                    Settings.PreviewFontFamily = value;
                    DebounceSave();
                }
            }
        }

        public string PreviewHotkey
        {
            get => Settings.PreviewHotkey;
            set
            {
                if (Settings.PreviewHotkey != value)
                {
                    Settings.PreviewHotkey = value;
                    OnPropertyChanged();
                    _controller.SaveSettings(Settings);
                }
            }
        }

        public string QuickPasteHotkey
        {
            get => Settings.QuickPasteHotkey;
            set
            {
                if (Settings.QuickPasteHotkey != value)
                {
                    Settings.QuickPasteHotkey = value;
                    OnPropertyChanged();
                    _controller.SaveSettings(Settings);
                }
            }
        }

        public string Theme
        {
            get => Settings.Theme;
            set
            {
                if (Settings.Theme != value)
                {
                    Settings.Theme = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public string LaunchPosition
        {
            get => Settings.LaunchPosition;
            set
            {
                if (Settings.LaunchPosition != value)
                {
                    Settings.LaunchPosition = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public bool StartWithWindows
        {
            get => Settings.StartWithWindows;
            set
            {
                if (Settings.StartWithWindows != value)
                {
                    Settings.StartWithWindows = value;
                    OnPropertyChanged();
                    _startupManagerService.SetStartup(value);
                    DebounceSave();
                }
            }
        }

        public string ClipItemPadding
        {
            get => Settings.ClipItemPadding;
            set
            {
                if (Settings.ClipItemPadding != value)
                {
                    Settings.ClipItemPadding = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public bool DisplayLogo
        {
            get => Settings.DisplayLogo;
            set
            {
                if (Settings.DisplayLogo != value)
                {
                    Settings.DisplayLogo = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public bool ShowHoverPreview
        {
            get => Settings.ShowHoverPreview;
            set
            {
                if (Settings.ShowHoverPreview != value)
                {
                    Settings.ShowHoverPreview = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public uint HoverPreviewDelay
        {
            get => Settings.HoverPreviewDelay;
            set
            {
                if (Settings.HoverPreviewDelay != value)
                {
                    Settings.HoverPreviewDelay = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public string AccentChromaLevel
        {
            get => Settings.AccentChromaLevel;
            set
            {
                if (Settings.AccentChromaLevel != value)
                {
                    Settings.AccentChromaLevel = value;
                    OnPropertyChanged();
                    UpdateAccentColor();
                    UpdateOklchHueBrush();
                }
            }
        }

        public double FontSize
        {
            get => Settings.FontSize;
            set
            {
                if (Settings.FontSize != value)
                {
                    Settings.FontSize = (float)value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public double PreviewFontSize
        {
            get => Settings.PreviewFontSize;
            set
            {
                if (Settings.PreviewFontSize != value)
                {
                    Settings.PreviewFontSize = (float)value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public double AccentHue
        {
            get => _accentHue;
            set
            {
                if (SetProperty(ref _accentHue, value))
                {
                    UpdateAccentColor();
                }
            }
        }

        public uint MaxClipsTotal
        {
            get => Settings.MaxClipsTotal;
            set
            {
                if (Settings.MaxClipsTotal != value)
                {
                    Settings.MaxClipsTotal = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public uint CleanupAgeDays
        {
            get => Settings.CleanupAgeDays;
            set
            {
                if (Settings.CleanupAgeDays != value)
                {
                    Settings.CleanupAgeDays = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public uint MaxClipSizeMb
        {
            get => Settings.MaxClipSizeMb;
            set
            {
                if (Settings.MaxClipSizeMb != value)
                {
                    Settings.MaxClipSizeMb = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public bool PasteAsPlainText
        {
            get => Settings.PasteAsPlainText;
            set
            {
                if (Settings.PasteAsPlainText != value)
                {
                    Settings.PasteAsPlainText = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public string LoggingLevel
        {
            get => Settings.LoggingLevel;
            set
            {
                if (Settings.LoggingLevel != value)
                {
                    Settings.LoggingLevel = value;
                    OnPropertyChanged();
                    DebounceSave();
                }
            }
        }

        public ICommand SaveSettingsCommand { get; }
        public ICommand ClearHistoryCommand { get; }
        public ICommand ClearCachesCommand { get; }
        public ICommand RunHeavyMaintenanceCommand { get; }
        public ICommand RemoveDeadheadClipsCommand { get; }
        public ICommand ClearOversizedCommand { get; }
        public ICommand ChangePageCommand { get; }
        public ICommand OpenGitHubUrlCommand { get; }
        public ICommand OpenSettingsFolderCommand { get; }
        public ICommand OpenTempDataFolderCommand { get; }
        public ICommand OpenAcknowledgementsWindowCommand { get; }
        public ICommand OpenExeFolderCommand { get; }
        public ICommand BrowseCompareToolCommand { get; }
        public ICommand AddSendToTargetCommand { get; }
        public ICommand RemoveSendToTargetCommand { get; }

        public SettingsViewModel(CliptooController controller, IContentDialogService contentDialogService, IStartupManagerService startupManagerService, IServiceProvider serviceProvider, IFontProvider fontProvider, IIconProvider iconProvider)
        {
            _controller = controller;
            _contentDialogService = contentDialogService;
            _startupManagerService = startupManagerService;
            _serviceProvider = serviceProvider;
            _fontProvider = fontProvider;
            _iconProvider = iconProvider;
            Settings = _controller.GetSettings();
            _selectedFontFamily = Settings.FontFamily;
            _selectedPreviewFontFamily = Settings.PreviewFontFamily;
            _oklchHueBrush = new SolidColorBrush(Colors.Transparent);
            AppVersion = System.Reflection.Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "0.0.0";

            var exePath = Process.GetCurrentProcess().MainModule?.FileName ?? "Not available";
            ExePathDir = Path.GetDirectoryName(exePath) ?? "Not available";
            ExePathFile = Path.GetFileName(exePath);

            SaveSettingsCommand = new RelayCommand(_ => _controller.SaveSettings(Settings));
            ClearHistoryCommand = new RelayCommand(async _ => await HandleClearHistory(), _ => !IsBusy);
            ClearCachesCommand = new RelayCommand(async _ =>
            {
                if (IsBusy) return;
                IsBusy = true;
                try
                {
                    await Task.Run(() => _controller.ClearCaches());
                    await ShowInformationDialogAsync("Caches Cleared", new System.Windows.Controls.TextBlock { Text = "All cached thumbnails and temporary files have been deleted." });
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
                        var result = await Task.Run(() => _controller.RunHeavyMaintenanceNowAsync());
                        await InitializeAsync();

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
                        if (results.Any())
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

                        await ShowInformationDialogAsync("Maintenance Complete", dialogContent);
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

            SettingsFolderPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "Cliptoo");
            TempDataPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Cliptoo");

            OpenGitHubUrlCommand = new RelayCommand(_ => Process.Start(new ProcessStartInfo(GitHubUrl) { UseShellExecute = true }));
            OpenSettingsFolderCommand = new RelayCommand(_ => Process.Start(new ProcessStartInfo(SettingsFolderPath) { UseShellExecute = true }));
            OpenTempDataFolderCommand = new RelayCommand(_ => Process.Start(new ProcessStartInfo(TempDataPath) { UseShellExecute = true }));
            OpenAcknowledgementsWindowCommand = new RelayCommand(_ => ShowAcknowledgementsWindow());
            OpenExeFolderCommand = new RelayCommand(_ => Process.Start(new ProcessStartInfo(ExePathDir) { UseShellExecute = true }));
            BrowseCompareToolCommand = new RelayCommand(_ => ExecuteBrowseCompareTool());
            AddSendToTargetCommand = new RelayCommand(_ => ExecuteAddSendToTarget());
            RemoveSendToTargetCommand = new RelayCommand(param => ExecuteRemoveSendToTarget(param as SendToTarget));

            SystemFonts = new ObservableCollection<string>();
            _ = PopulateFontsAsync();

            SendToTargets = new ObservableCollection<SendToTarget>(Settings.SendToTargets);
            foreach (var target in SendToTargets)
            {
                target.PropertyChanged += (s, e) => DebounceSave();
            }
            SendToTargets.CollectionChanged += (s, e) => DebounceSave();

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

        private async Task LoadIconsAsync()
        {
            LogoIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Logo, 138);
        }

        private void ExecuteBrowseCompareTool()
        {
            var openFileDialog = new OpenFileDialog
            {
                Filter = "Executable files (*.exe)|*.exe|All files (*.*)|*.*",
                Title = "Select a comparison program"
            };

            if (openFileDialog.ShowDialog() == true)
            {
                CompareToolPath = openFileDialog.FileName;
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
                Stats = await _controller.GetStatsAsync();
                OnPropertyChanged(nameof(StatsSummary));
            }
            catch (Exception ex)
            {
                Core.Configuration.LogManager.Log(ex, "Failed to load database stats.");
                Stats = new DbStats();
            }
        }

        private void OnDebounceTimerElapsed(object? sender, System.Timers.ElapsedEventArgs e)
        {
            Settings.SendToTargets = new List<SendToTarget>(SendToTargets);
            _controller.SaveSettings(Settings);
        }

        private void DebounceSave()
        {
            _saveDebounceTimer.Stop();
            _saveDebounceTimer.Start();
        }

        public void Cleanup()
        {
            _saveDebounceTimer.Elapsed -= OnDebounceTimerElapsed;
            _saveDebounceTimer.Dispose();
        }

        private async Task PopulateFontsAsync()
        {
            IsFontsLoading = true; var fonts = await Task.Run(() => { var fontList = new List<string> { "Source Code Pro" }; fontList.AddRange(Fonts.SystemFontFamilies.Select(f => f.Source).Distinct().OrderBy(s => s)); return fontList; });

            foreach (var fontName in fonts)
            {
                SystemFonts.Add(fontName);
            }

            SelectedFontFamily = SystemFonts.Contains(Settings.FontFamily) ? Settings.FontFamily : SystemFonts.First();
            SelectedPreviewFontFamily = SystemFonts.Contains(Settings.PreviewFontFamily) ? Settings.PreviewFontFamily : SystemFonts.First();
            IsFontsLoading = false;
        }

        private void UpdateAccentColor()
        {
            var currentTheme = ApplicationThemeManager.GetAppTheme();
            if (currentTheme == ApplicationTheme.Unknown)
            {
                currentTheme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
            }

            var lightness = currentTheme == ApplicationTheme.Dark ? 0.62 : 0.70;
            var hoverLightness = currentTheme == ApplicationTheme.Dark ? 0.68 : 0.64;
            var chroma = Settings.AccentChromaLevel == "vibrant" ? OKLCH_CHROMA_BRIGHT : OKLCH_CHROMA_MUTED;
            var hue = AccentHue;

            var (ar, ag, ab) = ColorParser.OklchToRgb(lightness, chroma, hue);
            var accentColor = System.Windows.Media.Color.FromRgb(ar, ag, ab);
            var accentBrush = new SolidColorBrush(accentColor);
            accentBrush.Freeze();

            var (hr, hg, hb) = ColorParser.OklchToRgb(hoverLightness, chroma, hue);
            var hoverColor = System.Windows.Media.Color.FromRgb(hr, hg, hb);
            var hoverBrush = new SolidColorBrush(hoverColor);
            hoverBrush.Freeze();

            Application.Current.Resources["AccentBrush"] = accentBrush;
            Application.Current.Resources["AccentBrushHover"] = hoverBrush;

            ApplicationAccentColorManager.Apply(accentColor);

            AccentBrush = accentBrush;
            Settings.AccentColor = $"#{accentColor.R:X2}{accentColor.G:X2}{accentColor.B:X2}";

            DebounceSave();
        }

        private void UpdateOklchHueBrush()
        {
            var gradientStops = new GradientStopCollection();
            var chroma = Settings.AccentChromaLevel == "vibrant" ? OKLCH_CHROMA_BRIGHT : OKLCH_CHROMA_MUTED;

            for (int i = 0; i <= 360; i += 10)
            {
                var (r, g, b) = ColorParser.OklchToRgb(OKLCH_LIGHTNESS, chroma, i);
                gradientStops.Add(new GradientStop(System.Windows.Media.Color.FromRgb(r, g, b), (double)i / 360.0));
            }
            var brush = new LinearGradientBrush(gradientStops, new Point(0, 0.5), new Point(1, 0.5));
            brush.Freeze();
            OklchHueBrush = brush;
        }

        private void InitializeAccentColor()
        {
            try
            {
                var color = (System.Windows.Media.Color)System.Windows.Media.ColorConverter.ConvertFromString(Settings.AccentColor);
                ColorParser.RgbToOklch(color.R, color.G, color.B, out _, out _, out var h);
                _accentHue = h;
                OnPropertyChanged(nameof(AccentHue));
                UpdateAccentColor();
            }
            catch (FormatException) { }
        }

        private async Task HandleClearHistory()
        {
            var viewModel = new ClearHistoryDialogViewModel();
            var dialog = new ContentDialog
            {
                Title = "Clear History?",
                Content = new Views.ClearHistoryDialog { DataContext = viewModel },
                PrimaryButtonText = "Delete",
                CloseButtonText = "Cancel"
            };

            var result = await _contentDialogService.ShowAsync(dialog, CancellationToken.None);

            if (result != ContentDialogResult.Primary)
            {
                viewModel.Result = ClearHistoryResult.Cancel;
            }
            else
            {
                viewModel.Result = viewModel.DeletePinned ? ClearHistoryResult.ClearAll : ClearHistoryResult.ClearUnpinned;
            }

            if (viewModel.Result == ClearHistoryResult.Cancel) return;

            if (IsBusy) return;
            IsBusy = true;
            try
            {
                if (viewModel.Result == ClearHistoryResult.ClearAll)
                {
                    await Task.Run(() => _controller.ClearAllHistoryAsync());
                }
                else if (viewModel.Result == ClearHistoryResult.ClearUnpinned)
                {
                    await Task.Run(() => _controller.ClearHistoryAsync());
                }
                await InitializeAsync();
            }
            finally
            {
                IsBusy = false;
            }
        }

        private async Task HandleClearOversized()
        {
            var viewModel = new ClearOversizedDialogViewModel();
            var dialog = new ContentDialog
            {
                Title = "Clear Oversized Clips?",
                Content = new Views.ClearOversizedDialog { DataContext = viewModel },
                PrimaryButtonText = "Delete",
                CloseButtonText = "Cancel"
            };

            var result = await _contentDialogService.ShowAsync(dialog, CancellationToken.None);

            if (result != ContentDialogResult.Primary)
            {
                return;
            }

            if (IsBusy) return;
            IsBusy = true;
            try
            {
                int count = await Task.Run(() => _controller.ClearOversizedClipsAsync(viewModel.SizeMb));
                await InitializeAsync();
                await ShowInformationDialogAsync("Oversized Clips Removed", new System.Windows.Controls.TextBlock { Text = $"{count} clip(s) larger than {viewModel.SizeMb} MB have been removed." });
            }
            finally
            {
                IsBusy = false;
            }
        }

        private async Task HandleRemoveDeadheadClips()
        {
            if (IsBusy) return;
            IsBusy = true;
            try
            {
                int count = await Task.Run(() => _controller.RemoveDeadheadClipsAsync());
                await InitializeAsync();
                await ShowInformationDialogAsync("Deadhead Clips Removed", new System.Windows.Controls.TextBlock { Text = $"{count} clip(s) pointing to non-existent files or folders have been removed." });
            }
            finally
            {
                IsBusy = false;
            }
        }

        private async Task ShowInformationDialogAsync(string title, UIElement content)
        {
            var dialog = new ContentDialog
            {
                Title = title,
                Content = content,
                CloseButtonText = "OK"
            };
            await _contentDialogService.ShowAsync(dialog, CancellationToken.None);
        }

        private void ShowAcknowledgementsWindow()
        {
            var window = _serviceProvider.GetRequiredService<AcknowledgementsWindow>();
            window.Owner = Application.Current.Windows.OfType<SettingsWindow>().FirstOrDefault();
            window.ShowDialog();
        }

        public void UpdateHotkey(KeyEventArgs e, string target)
        {
            if (!IsCapturingHotkey || CapturingHotkeyTarget != target) return;
            e.Handled = true;

            var key = (e.Key == Key.System) ? e.SystemKey : e.Key;

            if (key is Key.Back or Key.Delete)
            {
                switch (target)
                {
                    case "Main": this.Hotkey = string.Empty; break;
                    case "Preview": this.PreviewHotkey = string.Empty; break;
                    case "QuickPaste": this.QuickPasteHotkey = string.Empty; break;
                }
                return;
            }

            bool isModifierKey = key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin or Key.None;

            if (target != "QuickPaste" && isModifierKey)
            {
                return;
            }

            var hotkeyParts = new List<string>();
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Control)) hotkeyParts.Add("Ctrl");
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Alt)) hotkeyParts.Add("Alt");
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Shift)) hotkeyParts.Add("Shift");

            if (!isModifierKey)
            {
                hotkeyParts.Add(key.ToString());
            }

            var newHotkey = string.Join("+", hotkeyParts);

            switch (target)
            {
                case "Main":
                    this.Hotkey = newHotkey;
                    break;
                case "Preview":
                    this.PreviewHotkey = newHotkey;
                    break;
                case "QuickPaste":
                    this.QuickPasteHotkey = newHotkey;
                    break;
            }
        }

        public async void ValidateHotkey(string target)
        {
            if (target == "QuickPaste")
            {
                var parts = this.QuickPasteHotkey.Split(new[] { '+' }, StringSplitOptions.RemoveEmptyEntries);
                bool allModifiers = parts.All(p => p is "Ctrl" or "Alt" or "Shift" or "Win");

                if (!allModifiers || parts.Length < 2)
                {
                    this.QuickPasteHotkey = "Ctrl+Alt";
                    var dialog = new ContentDialog
                    {
                        Title = "Invalid Hotkey",
                        Content = "The Quick Paste hotkey must consist of at least two modifier keys (e.g., Ctrl, Alt, Shift). It has been reset to the default 'Ctrl+Alt'.",
                        CloseButtonText = "OK"
                    };
                    await _contentDialogService.ShowAsync(dialog, CancellationToken.None);
                }
            }
        }

        private void ExecuteAddSendToTarget()
        {
            var openFileDialog = new OpenFileDialog
            {
                Filter = "Executable files (*.exe)|*.exe|All files (*.*)|*.*",
                Title = "Select an Application"
            };

            if (openFileDialog.ShowDialog() == true)
            {
                var newTarget = new SendToTarget
                {
                    Path = openFileDialog.FileName,
                    Name = Path.GetFileNameWithoutExtension(openFileDialog.FileName),
                    Arguments = "\"{0}\""
                };
                (newTarget as INotifyPropertyChanged).PropertyChanged += (s, e) => DebounceSave();
                SendToTargets.Add(newTarget);
            }
        }

        private void ExecuteRemoveSendToTarget(SendToTarget? target)
        {
            if (target != null)
            {
                SendToTargets.Remove(target);
            }
        }
    }

    public enum ClearHistoryResult { Cancel, ClearUnpinned, ClearAll }

    public class ClearHistoryDialogViewModel : ViewModelBase
    {
        private bool _deletePinned;
        public bool DeletePinned { get => _deletePinned; set => SetProperty(ref _deletePinned, value); }
        public ClearHistoryResult Result { get; set; } = ClearHistoryResult.Cancel;
    }

}