using System.Windows;

namespace Cliptoo.UI.Services
{
    public interface IThemeService
    {
        void Initialize(Window window);
        void ApplyThemeFromSettings();
        void ApplyAccentColor(double hue);
    }
}