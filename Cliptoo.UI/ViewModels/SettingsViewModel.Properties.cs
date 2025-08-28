using System.Collections.ObjectModel;
using System.Windows.Media;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;

namespace Cliptoo.UI.ViewModels
{
    internal partial class SettingsViewModel
    {
        private bool _isBusy;
        private ImageSource? _logoIcon;
        private bool _isFontsLoading = true;

        public Settings Settings { get => _settings; set => SetProperty(ref _settings, value); }
        public DbStats Stats { get => _stats; set => SetProperty(ref _stats, value); }
        public SolidColorBrush AccentBrush { get => _accentBrush; set => SetProperty(ref _accentBrush, value); }
        public Brush OklchHueBrush { get => _oklchHueBrush; private set => SetProperty(ref _oklchHueBrush, value); }
        public string CurrentPage { get => _currentPage; set => SetProperty(ref _currentPage, value); }
        public bool IsBusy { get => _isBusy; set => SetProperty(ref _isBusy, value); }
        public ImageSource? LogoIcon { get => _logoIcon; private set => SetProperty(ref _logoIcon, value); }
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
                    _controller.SaveSettings();
                }
            }
        }

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
                    _controller.SaveSettings();
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
                    _controller.SaveSettings();
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
    }
}