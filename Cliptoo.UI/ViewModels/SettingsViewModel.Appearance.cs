using System.Windows;
using System.Windows.Media;
using Cliptoo.Core.Services;
using Wpf.Ui.Appearance;

namespace Cliptoo.UI.ViewModels
{
    internal partial class SettingsViewModel
    {
        private void UpdateOklchHueBrush()
        {
            var currentTheme = ApplicationThemeManager.GetAppTheme();
            if (currentTheme == ApplicationTheme.Unknown)
            {
                currentTheme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
            }

            var lightness = currentTheme == ApplicationTheme.Dark ? 0.50 : 0.75;
            var gradientStops = new GradientStopCollection();
            var chromaProportion = ColorParser.GetChromaFromLevel(Settings.AccentChromaLevel);

            for (int i = 0; i <= 360; i += 5)
            {
                var maxChromaForHue = ColorParser.FindMaxChroma(lightness, i);
                var finalChroma = maxChromaForHue * chromaProportion;
                var (r, g, b) = ColorParser.OklchToRgb(lightness, finalChroma, i);
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
                var color = (System.Windows.Media.Color)ColorConverter.ConvertFromString(Settings.AccentColor);
                ColorParser.RgbToOklch(color.R, color.G, color.B, out _, out _, out var h);
                _accentHue = h;
                OnPropertyChanged(nameof(AccentHue));
                AccentBrush = (SolidColorBrush)Application.Current.Resources["AccentBrush"];
            }
            catch (FormatException) { }
        }
    }
}