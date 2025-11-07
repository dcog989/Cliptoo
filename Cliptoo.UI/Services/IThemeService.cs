using System.Windows;

namespace Cliptoo.UI.Services
{
    internal interface IThemeService
    {
        void Initialize(Window window);
        void ApplyThemeFromSettings();
        void ApplyAccentColor(double hue);
        event EventHandler? ThemeChanged;
    }
}