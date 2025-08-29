using System.Windows;
using System.Windows.Media;
using Cliptoo.Core.Services;
using Wpf.Ui.Appearance;

namespace Cliptoo.UI.ViewModels
{
    internal partial class SettingsViewModel
    {
        private const double OKLCH_LIGHTNESS = 0.63;
        private const double OKLCH_CHROMA_BRIGHT = 0.22;
        private const double OKLCH_CHROMA_MUTED = 0.10;

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
    }
}