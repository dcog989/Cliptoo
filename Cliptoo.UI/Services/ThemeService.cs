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

        private const double ACCENT_LIGHTNESS_DARK_THEME = 0.45;
        private const double HOVER_LIGHTNESS_DARK_THEME = 0.55;
        private const double ACCENT_LIGHTNESS_LIGHT_THEME = 0.85;
        private const double HOVER_LIGHTNESS_LIGHT_THEME = 0.95;
        private const double SELECTED_HIGHLIGHT_LIGHTNESS_OFFSET = -0.20;

        private const double ACCENT_TEXT_LIGHTNESS_DARK_THEME = 0.75;
        private const double ACCENT_TEXT_LIGHTNESS_LIGHT_THEME = 0.45;

        private const double LUMINANCE_R_COEFFICIENT = 0.299;
        private const double LUMINANCE_G_COEFFICIENT = 0.587;
        private const double LUMINANCE_B_COEFFICIENT = 0.114;
        private const int LUMINANCE_THRESHOLD = 128;


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

                var lightness = currentTheme == ApplicationTheme.Dark ? ACCENT_LIGHTNESS_DARK_THEME : ACCENT_LIGHTNESS_LIGHT_THEME;
                var hoverLightness = currentTheme == ApplicationTheme.Dark ? HOVER_LIGHTNESS_DARK_THEME : HOVER_LIGHTNESS_LIGHT_THEME;
                var textLightness = currentTheme == ApplicationTheme.Dark ? ACCENT_TEXT_LIGHTNESS_DARK_THEME : ACCENT_TEXT_LIGHTNESS_LIGHT_THEME;
                var chromaProportion = ColorParser.GetChromaFromLevel(settings.AccentChromaLevel);

                // --- Main Accent Color ---
                var maxChroma = ColorParser.FindMaxChroma(lightness, hue);
                var finalChroma = maxChroma * chromaProportion;
                var (ar, ag, ab) = ColorParser.OklchToRgb(lightness, finalChroma, hue);
                var accentColor = Color.FromRgb(ar, ag, ab);
                var accentBrush = new SolidColorBrush(accentColor);
                accentBrush.Freeze();

                // --- Hover Color ---
                var maxHoverChroma = ColorParser.FindMaxChroma(hoverLightness, hue);
                var finalHoverChroma = maxHoverChroma * chromaProportion;
                var (hr, hg, hb) = ColorParser.OklchToRgb(hoverLightness, finalHoverChroma, hue);
                var hoverColor = Color.FromRgb(hr, hg, hb);
                var hoverBrush = new SolidColorBrush(hoverColor);
                hoverBrush.Freeze();

                // --- Selected Highlight Color ---
                var selectedHighlightLightness = lightness + SELECTED_HIGHLIGHT_LIGHTNESS_OFFSET;
                var maxSelectedChroma = ColorParser.FindMaxChroma(selectedHighlightLightness, hue);
                var finalSelectedChroma = maxSelectedChroma * chromaProportion;
                var (shr, shg, shb) = ColorParser.OklchToRgb(selectedHighlightLightness, finalSelectedChroma, hue);
                var selectedHighlightColor = Color.FromRgb(shr, shg, shb);
                var selectedHighlightBrush = new SolidColorBrush(selectedHighlightColor);
                selectedHighlightBrush.Freeze();

                // --- Accent Text Color ---
                var maxTextChroma = ColorParser.FindMaxChroma(textLightness, hue);
                var finalTextChroma = maxTextChroma * chromaProportion;
                var (tr, tg, tb) = ColorParser.OklchToRgb(textLightness, finalTextChroma, hue);
                var accentTextColor = Color.FromRgb(tr, tg, tb);
                var accentTextBrush = new SolidColorBrush(accentTextColor);
                accentTextBrush.Freeze();

                Application.Current.Resources["AccentBrush"] = accentBrush;
                Application.Current.Resources["AccentBrushHover"] = hoverBrush;
                Application.Current.Resources["AccentBrushSelectedHighlight"] = selectedHighlightBrush;
                Application.Current.Resources["AccentTextBrush"] = accentTextBrush;

                // First, let the library apply its default (and potentially incorrect) styling.
                ApplicationAccentColorManager.Apply(accentColor);

                // Then, immediately override the text-on-accent color with our own calculated high-contrast color.
                var brightness = (accentColor.R * LUMINANCE_R_COEFFICIENT) + (accentColor.G * LUMINANCE_G_COEFFICIENT) + (accentColor.B * LUMINANCE_B_COEFFICIENT);
                var textOnAccentBrush = brightness > LUMINANCE_THRESHOLD
                    ? new SolidColorBrush(Colors.Black)
                    : new SolidColorBrush(Colors.White);
                textOnAccentBrush.Freeze();
                Application.Current.Resources["TextOnAccentFillColorPrimaryBrush"] = textOnAccentBrush;

                settings.AccentColor = $"#{accentColor.R:X2}{accentColor.G:X2}{accentColor.B:X2}";
            }
            catch (FormatException ex)
            {
                LogManager.LogCritical(ex, $"Invalid accent color format in settings: {settings.AccentColor}");
            }
        }

    }
}