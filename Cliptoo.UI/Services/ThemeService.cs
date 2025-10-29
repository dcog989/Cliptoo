using System.Windows;
using System.Windows.Media;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using Wpf.Ui.Appearance;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Services
{
    internal sealed class ThemeService : IThemeService
    {
        private readonly ISettingsService _settingsService;
        private Window? _window;

        public ThemeService(ISettingsService settingsService)
        {
            _settingsService = settingsService;
        }

        public void Initialize(Window window)
        {
            _window = window;
            ApplyThemeFromSettings();
        }

        public void ApplyThemeFromSettings()
        {
            var settings = _settingsService.Settings;
            if (_window is null || !_window.IsLoaded || new System.Windows.Interop.WindowInteropHelper(_window).Handle == System.IntPtr.Zero)
            {
                return;
            }

            var wpfuiTheme = settings.Theme?.ToLowerInvariant() switch
            {
                "light" => ApplicationTheme.Light,
                "dark" => ApplicationTheme.Dark,
                _ => ApplicationTheme.Unknown,
            };

            SystemThemeWatcher.UnWatch(_window);

            var finalTheme = wpfuiTheme;
            if (wpfuiTheme == ApplicationTheme.Unknown)
            {
                var systemTheme = ApplicationThemeManager.GetSystemTheme();
                finalTheme = systemTheme == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
                ApplicationThemeManager.Apply(finalTheme, WindowBackdropType.Mica, false);
                SystemThemeWatcher.Watch(_window, WindowBackdropType.Mica, false);
            }
            else
            {
                ApplicationThemeManager.Apply(wpfuiTheme, WindowBackdropType.Mica, false);
            }

            if (finalTheme == ApplicationTheme.Dark)
            {
                Application.Current.Resources["HyperlinkBlueBrush"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#5599FF"));
                Application.Current.Resources["HyperlinkBlueBrushHover"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#88BBFF"));
            }
            else
            {
                Application.Current.Resources["HyperlinkBlueBrush"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#0078D4"));
                Application.Current.Resources["HyperlinkBlueBrushHover"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#005A9E"));
            }

            try
            {
                var color = (Color)ColorConverter.ConvertFromString(settings.AccentColor);
                ColorParser.RgbToOklch(color.R, color.G, color.B, out _, out _, out var h);
                ApplyAccentColor(h);
            }
            catch (FormatException)
            {
                ApplyAccentColor(240); // Default blue hue
            }
        }

        public void ApplyAccentColor(double hue)
        {
            var settings = _settingsService.Settings;
            try
            {
                var currentTheme = ApplicationThemeManager.GetAppTheme();
                if (currentTheme == ApplicationTheme.Unknown)
                {
                    currentTheme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
                }

                var lightness = currentTheme == ApplicationTheme.Dark ? 0.62 : 0.70;
                var hoverLightness = currentTheme == ApplicationTheme.Dark ? 0.68 : 0.64;
                var chroma = ColorParser.GetChromaFromLevel(settings.AccentChromaLevel);

                var (ar, ag, ab) = ColorParser.OklchToRgb(lightness, chroma, hue);
                var accentColor = Color.FromRgb(ar, ag, ab);
                var accentBrush = new SolidColorBrush(accentColor);
                accentBrush.Freeze();

                var (hr, hg, hb) = ColorParser.OklchToRgb(hoverLightness, chroma, hue);
                var hoverColor = Color.FromRgb(hr, hg, hb);
                var hoverBrush = new SolidColorBrush(hoverColor);
                hoverBrush.Freeze();

                var selectedHighlightLightness = lightness - 0.20;
                var (shr, shg, shb) = ColorParser.OklchToRgb(selectedHighlightLightness, chroma, hue);
                var selectedHighlightColor = Color.FromRgb(shr, shg, shb);
                var selectedHighlightBrush = new SolidColorBrush(selectedHighlightColor);
                selectedHighlightBrush.Freeze();

                Application.Current.Resources["AccentBrush"] = accentBrush;
                Application.Current.Resources["AccentBrushHover"] = hoverBrush;
                Application.Current.Resources["AccentBrushSelectedHighlight"] = selectedHighlightBrush;

                // Dynamically set text color for contrast
                var brightness = (accentColor.R * 0.299) + (accentColor.G * 0.587) + (accentColor.B * 0.114);
                var textOnAccentBrush = brightness > 128
                    ? new SolidColorBrush(Colors.Black)
                    : new SolidColorBrush(Colors.White);
                textOnAccentBrush.Freeze();
                Application.Current.Resources["TextOnAccentFillColorPrimaryBrush"] = textOnAccentBrush;

                ApplicationAccentColorManager.Apply(accentColor);

                settings.AccentColor = $"#{accentColor.R:X2}{accentColor.G:X2}{accentColor.B:X2}";
            }
            catch (FormatException ex)
            {
                LogManager.LogCritical(ex, $"Invalid accent color format in settings: {settings.AccentColor}");
            }
        }

        private static double GetChromaFromLevel(string level)
        {
            return level?.ToLowerInvariant() switch
            {
                "neon" => 0.28,
                "vibrant" => 0.22,
                "mellow" => 0.16,
                "muted" => 0.10,
                "ditchwater" => 0.05,
                _ => 0.22, // Default to Vibrant
            };
        }
    }
}