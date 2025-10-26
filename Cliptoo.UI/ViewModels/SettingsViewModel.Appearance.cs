using System.Windows;
using System.Windows.Media;
using Cliptoo.Core.Services;

namespace Cliptoo.UI.ViewModels
{
    internal partial class SettingsViewModel
    {
        private const double OKLCH_LIGHTNESS = 0.63;

        private static double GetChromaFromLevel(string level)
        {
            return level?.ToLowerInvariant() switch
            {
                "neon" => 0.28,
                "vibrant" => 0.22,
                "mellow" => 0.16,
                "muted" => 0.10,
                "ditchwater" => 0.05,
                _ => 0.22,
            };
        }

        private void UpdateOklchHueBrush()
        {
            var gradientStops = new GradientStopCollection();
            var chroma = GetChromaFromLevel(Settings.AccentChromaLevel);

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