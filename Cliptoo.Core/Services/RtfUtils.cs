using System;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;

namespace Cliptoo.Core.Services
{
    public static class RtfUtils
    {
        public static string ToPlainText(string rtf)
        {
            if (string.IsNullOrEmpty(rtf)) return string.Empty;

            if (Application.Current?.Dispatcher == null)
            {
                return string.Empty;
            }

            if (!Application.Current.Dispatcher.CheckAccess())
            {
                return Application.Current.Dispatcher.Invoke(() => ToPlainText(rtf));
            }

            var richTextBox = new RichTextBox();
            var range = new TextRange(richTextBox.Document.ContentStart, richTextBox.Document.ContentEnd);
            using (var stream = new MemoryStream(Encoding.UTF8.GetBytes(rtf)))
            {
                try
                {
                    range.Load(stream, DataFormats.Rtf);
                }
                catch (ArgumentException)
                {
                    return string.Empty;
                }
            }
            return range.Text.TrimEnd('\r', '\n');
        }
    }
}