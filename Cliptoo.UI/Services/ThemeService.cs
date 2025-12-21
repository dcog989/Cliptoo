using System.Windows;
using System.Windows.Media;
using Cliptoo.Core.Interfaces;
using Cliptoo.Core.Logging;
using Cliptoo.Core.Services;
using Wpf.Ui.Appearance;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Services
{
    public sealed class ThemeService : IThemeService
    {
        private readonly ISettingsService _settingsService;
        private Window? _window;
        private bool _isProcessing;

        private const double ACCENT_LIGHTNESS_DARK_THEME = 0.45;
        private const double HOVER_LIGHTNESS_DARK_THEME = 0.55;
        private const double ACCENT_TEXT_LIGHTNESS_DARK_THEME = 0.75;

        private const double ACCENT_LIGHTNESS_LIGHT_THEME = 0.85;
        private const double HOVER_LIGHTNESS_LIGHT_THEME = 0.95;
        private const double ACCENT_TEXT_LIGHTNESS_LIGHT_THEME = 0.45;

        private const double SELECTED_HIGHLIGHT_LIGHTNESS_OFFSET = -0.20;

        private const double LUMINANCE_R_COEFFICIENT = 0.299;
        private const double LUMINANCE_G_COEFFICIENT = 0.587;
        private const double LUMINANCE_B_COEFFICIENT = 0.114;
        private const int LUMINANCE_THRESHOLD = 128;

        public event EventHandler? ThemeChanged;

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
            if (_isProcessing) return;

            Application.Current.Dispatcher.VerifyAccess();

            var settings = _settingsService.Settings;
            if (_window is null || !_window.IsLoaded || new System.Windows.Interop.WindowInteropHelper(_window).Handle == IntPtr.Zero)
            {
                return;
            }

            _isProcessing = true;
            try
            {
                var wpfuiTheme = settings.Theme?.ToUpperInvariant() switch
                {
                    "LIGHT" => ApplicationTheme.Light,
                    "DARK" => ApplicationTheme.Dark,
                    _ => ApplicationTheme.Unknown,
                };

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

                var resources = Application.Current.Resources;
                if (finalTheme == ApplicationTheme.Dark)
                {
                    resources["HyperlinkBlueBrush"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#5599FF"));
                    resources["HyperlinkBlueBrushHover"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#88BBFF"));
                    resources["TagTextBrush"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#aaaaaa"));
                }
                else
                {
                    resources["HyperlinkBlueBrush"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#0078D4"));
                    resources["HyperlinkBlueBrushHover"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#005A9E"));
                    resources["TagTextBrush"] = new SolidColorBrush((Color)ColorConverter.ConvertFromString("#666666"));
                }

                try
                {
                    var color = (Color)ColorConverter.ConvertFromString(settings.AccentColor);
                    ColorParser.RgbToOklch(color.R, color.G, color.B, out _, out _, out var h);
                    InternalApplyAccentColor(h);
                }
                catch (FormatException)
                {
                    InternalApplyAccentColor(240);
                }

                ThemeChanged?.Invoke(this, EventArgs.Empty);
            }
            finally
            {
                _isProcessing = false;
            }
        }

        public void ApplyAccentColor(double hue)
        {
            if (_isProcessing) return;

            if (!Application.Current.Dispatcher.CheckAccess())
            {
                Application.Current.Dispatcher.Invoke(() => ApplyAccentColor(hue));
                return;
            }

            _isProcessing = true;
            try
            {
                InternalApplyAccentColor(hue);
                ThemeChanged?.Invoke(this, EventArgs.Empty);
            }
            finally
            {
                _isProcessing = false;
            }
        }

        private void InternalApplyAccentColor(double hue)
        {
            LogManager.LogDebug($"THEME_DIAG: InternalApplyAccentColor starting. Hue: {hue:F2}");
            var settings = _settingsService.Settings;
            var currentTheme = ApplicationThemeManager.GetAppTheme();
            if (currentTheme == ApplicationTheme.Unknown)
            {
                currentTheme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
            }

            var isDark = currentTheme == ApplicationTheme.Dark;
            var lightness = isDark ? ACCENT_LIGHTNESS_DARK_THEME : ACCENT_LIGHTNESS_LIGHT_THEME;
            var hoverLightness = isDark ? HOVER_LIGHTNESS_DARK_THEME : HOVER_LIGHTNESS_LIGHT_THEME;
            var textLightness = isDark ? ACCENT_TEXT_LIGHTNESS_DARK_THEME : ACCENT_TEXT_LIGHTNESS_LIGHT_THEME;
            var chromaProportion = ColorParser.GetChromaFromLevel(settings.AccentChromaLevel);

            var maxChroma = ColorParser.FindMaxChroma(lightness, hue);
            var (pr, pg, pb) = ColorParser.OklchToRgb(lightness, maxChroma * chromaProportion, hue);
            var colorPrimary = Color.FromRgb(pr, pg, pb);

            var maxHoverChroma = ColorParser.FindMaxChroma(hoverLightness, hue);
            var (hr, hg, hb) = ColorParser.OklchToRgb(hoverLightness, maxHoverChroma * chromaProportion, hue);
            var colorSecondary = Color.FromRgb(hr, hg, hb);

            var selectedHighlightLightness = lightness + SELECTED_HIGHLIGHT_LIGHTNESS_OFFSET;
            var maxSelectedChroma = ColorParser.FindMaxChroma(selectedHighlightLightness, hue);
            var (shr, shg, shb) = ColorParser.OklchToRgb(selectedHighlightLightness, maxSelectedChroma * chromaProportion, hue);
            var colorTertiary = Color.FromRgb(shr, shg, shb);

            var maxTextChroma = ColorParser.FindMaxChroma(textLightness, hue);
            var (tr, tg, tb) = ColorParser.OklchToRgb(textLightness, maxTextChroma * chromaProportion, hue);
            var colorText = Color.FromRgb(tr, tg, tb);

            ApplicationAccentColorManager.Apply(colorPrimary, currentTheme);

            var resources = Application.Current.Resources;

            void SetBrush(string key, Color color)
            {
                var brush = new SolidColorBrush(color);
                brush.Freeze();
                resources[key] = brush;
            }

            // Core Color resources (Template and Palette levels)
            resources["SystemAccentColor"] = colorPrimary;
            resources["SystemAccentColorPrimary"] = colorPrimary;
            resources["SystemAccentColorSecondary"] = colorSecondary;
            resources["SystemAccentColorTertiary"] = colorTertiary;
            resources["AccentFillColorDefault"] = colorPrimary;
            resources["AccentFillColorSecondary"] = colorSecondary;
            resources["AccentFillColorTertiary"] = colorTertiary;

            // Accent Fill Brushes (Primary Buttons, Checkboxes, etc)
            SetBrush("AccentFillColorDefaultBrush", colorPrimary);
            SetBrush("AccentFillColorSecondaryBrush", colorSecondary);
            SetBrush("AccentFillColorTertiaryBrush", colorTertiary);
            SetBrush("AccentFillColorValueBrush", colorPrimary);

            // Control Accent keys (Exhaustive set for WPF-UI v4 Button Primary)
            SetBrush("ControlAccentFillColorDefaultBrush", colorPrimary);
            SetBrush("ControlAccentFillColorSecondaryBrush", colorSecondary);
            SetBrush("ControlAccentFillColorTertiaryBrush", colorTertiary);

            // General Control keys
            SetBrush("ControlFillColorPrimaryBrush", colorPrimary);
            SetBrush("ControlStrongFillColorDefaultBrush", colorPrimary);
            SetBrush("ControlStrongStrokeColorDefaultBrush", colorPrimary);

            // Interaction and Focus keys
            SetBrush("SystemFillColorAttentionBrush", colorPrimary);
            SetBrush("ControlStrokeColorOnAccentDefaultBrush", colorPrimary);
            SetBrush("ControlStrokeColorOnAccentSecondaryBrush", colorSecondary);
            SetBrush("ControlStrokeColorOnAccentTertiaryBrush", colorTertiary);

            // Selection and Text keys
            resources["TextControlSelectionHighlightColor"] = colorPrimary;
            SetBrush("TextControlSelectionHighlightBrush", colorPrimary);
            SetBrush("AccentTextFillColorPrimaryBrush", colorText);
            SetBrush("AccentTextFillColorSecondaryBrush", colorText);
            SetBrush("AccentTextFillColorTertiaryBrush", colorText);

            // App-internal keys
            SetBrush("AccentBrush", colorPrimary);
            SetBrush("AccentBrushHover", colorSecondary);
            SetBrush("AccentBrushSelectedHighlight", colorTertiary);
            SetBrush("AccentTextBrush", colorText);

            var brightness = (colorPrimary.R * LUMINANCE_R_COEFFICIENT) + (colorPrimary.G * LUMINANCE_G_COEFFICIENT) + (colorPrimary.B * LUMINANCE_B_COEFFICIENT);
            var textOnAccentColor = brightness > LUMINANCE_THRESHOLD ? Colors.Black : Colors.White;
            SetBrush("TextOnAccentFillColorPrimaryBrush", textOnAccentColor);

            var newHex = $"#{colorPrimary.R:X2}{colorPrimary.G:X2}{colorPrimary.B:X2}";
            if (!string.Equals(settings.AccentColor, newHex, StringComparison.OrdinalIgnoreCase))
            {
                settings.AccentColor = newHex;
            }

            LogManager.LogDebug($"THEME_DIAG: Accent update complete. Primary set to {colorPrimary}");
        }

    }
}
