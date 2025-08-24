using System.Collections.Concurrent;
using System.Windows.Media;

namespace Cliptoo.UI.Services
{
    public class FontProvider : IFontProvider
    {
        private readonly ConcurrentDictionary<string, FontFamily> _fontCache = new();

        public FontProvider()
        {
            var sourceCodePro = new FontFamily(new Uri("pack://application:,,,/"), "./Assets/Fonts/#Source Code Pro");
            _fontCache.TryAdd("Source Code Pro", sourceCodePro);
        }

        public FontFamily GetFont(string fontFamilyName)
        {
            if (string.IsNullOrEmpty(fontFamilyName))
            {
                fontFamilyName = "Segoe UI";
            }

            return _fontCache.GetOrAdd(fontFamilyName, name => new FontFamily(name));
        }
    }
}