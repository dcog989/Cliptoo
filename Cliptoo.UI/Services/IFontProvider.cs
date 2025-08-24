using System.Windows.Media;

namespace Cliptoo.UI.Services
{
    public interface IFontProvider
    {
        FontFamily GetFont(string fontFamilyName);
    }
}