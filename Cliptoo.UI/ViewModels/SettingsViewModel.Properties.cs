using System.Collections.ObjectModel;
using System.Windows;
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
            set => Settings.CompareToolPath = value;
        }

        public string Hotkey
        {
            get => Settings.Hotkey;
            set
            {
                if (Settings.Hotkey != value)
                {
                    Settings.Hotkey = value;
                    LogManager.Log($"Setting 'Main hotkey' changed to: {value}");
                    _settingsService.SaveSettings();
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
                    LogManager.Log($"Setting 'Preview hotkey' changed to: {value}");
                    _settingsService.SaveSettings();
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
                    LogManager.Log($"Setting 'Quick Paste hotkey' changed to: {value}");
                    _settingsService.SaveSettings();
                }
            }
        }

        public string Theme
        {
            get => Settings.Theme;
            set => Settings.Theme = value;
        }

        public string LaunchPosition
        {
            get => Settings.LaunchPosition;
            set => Settings.LaunchPosition = value;
        }

        public bool StartWithWindows
        {
            get => Settings.StartWithWindows;
            set => Settings.StartWithWindows = value;
        }

        public string ClipItemPadding
        {
            get => Settings.ClipItemPadding;
            set => Settings.ClipItemPadding = value;
        }

        public bool DisplayLogo
        {
            get => Settings.DisplayLogo;
            set => Settings.DisplayLogo = value;
        }

        public bool ShowHoverPreview
        {
            get => Settings.ShowHoverPreview;
            set => Settings.ShowHoverPreview = value;
        }

        public uint HoverPreviewDelay
        {
            get => Settings.HoverPreviewDelay;
            set => Settings.HoverPreviewDelay = value;
        }

        public string AccentChromaLevel
        {
            get => Settings.AccentChromaLevel;
            set => Settings.AccentChromaLevel = value;
        }

        public double FontSize
        {
            get => Settings.FontSize;
            set => Settings.FontSize = (float)value;
        }

        public double PreviewFontSize
        {
            get => Settings.PreviewFontSize;
            set => Settings.PreviewFontSize = (float)value;
        }

        public double AccentHue
        {
            get => _accentHue;
            set
            {
                if (SetProperty(ref _accentHue, value))
                {
                    _themeService.ApplyAccentColor(value);
                    AccentBrush = (SolidColorBrush)Application.Current.Resources["AccentBrush"];
                }
            }
        }

        public uint MaxClipsTotal
        {
            get => Settings.MaxClipsTotal;
            set => Settings.MaxClipsTotal = value;
        }

        public uint CleanupAgeDays
        {
            get => Settings.CleanupAgeDays;
            set => Settings.CleanupAgeDays = value;
        }

        public uint MaxClipSizeMb
        {
            get => Settings.MaxClipSizeMb;
            set => Settings.MaxClipSizeMb = value;
        }

        public bool PasteAsPlainText
        {
            get => Settings.PasteAsPlainText;
            set => Settings.PasteAsPlainText = value;
        }

        public string LoggingLevel
        {
            get => Settings.LoggingLevel;
            set => Settings.LoggingLevel = value;
        }
    }
}