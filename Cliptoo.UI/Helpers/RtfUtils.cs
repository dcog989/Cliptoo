using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;

namespace Cliptoo.UI.Helpers
{
    public static class RtfUtils
    {
        public static string ToPlainText(string rtf)
        {
            if (string.IsNullOrEmpty(rtf)) return string.Empty;

            var richTextBox = new RichTextBox();
            var range = new TextRange(richTextBox.Document.ContentStart, richTextBox.Document.ContentEnd);
            using (var stream = new MemoryStream(Encoding.UTF8.GetBytes(rtf)))
            {
                try
                {
                    range.Load(stream, DataFormats.Rtf);
                }
                catch (System.Exception)
                {
                    // Fallback for invalid RTF
                    return string.Empty;
                }
            }
            return range.Text.TrimEnd('\r', '\n');
        }
    }
}