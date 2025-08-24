using System.ComponentModel;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Documents;
using System.Windows.Media;

namespace Cliptoo.UI.Helpers
{
    public static class TextBlockHighlighter
    {
        public static readonly DependencyProperty FormattedTextProperty =
            DependencyProperty.RegisterAttached(
                "FormattedText",
                typeof(string),
                typeof(TextBlockHighlighter),
                new PropertyMetadata(null, OnFormattedTextChanged));

        public static string GetFormattedText(DependencyObject obj)
        {
            return (string)obj.GetValue(FormattedTextProperty);
        }

        public static void SetFormattedText(DependencyObject obj, string value)
        {
            obj.SetValue(FormattedTextProperty, value);
        }

        private static void OnFormattedTextChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is TextBlock textBlock)
            {
                // Unsubscribe from previous property changes to avoid memory leaks
                var oldDescriptorFamily = DependencyPropertyDescriptor.FromProperty(TextBlock.FontFamilyProperty, typeof(TextBlock));
                oldDescriptorFamily?.RemoveValueChanged(textBlock, OnFontPropertyChanged);

                var oldDescriptorSize = DependencyPropertyDescriptor.FromProperty(TextBlock.FontSizeProperty, typeof(TextBlock));
                oldDescriptorSize?.RemoveValueChanged(textBlock, OnFontPropertyChanged);

                UpdateInlines(textBlock);

                // Subscribe to future property changes
                var newDescriptorFamily = DependencyPropertyDescriptor.FromProperty(TextBlock.FontFamilyProperty, typeof(TextBlock));
                newDescriptorFamily?.AddValueChanged(textBlock, OnFontPropertyChanged);

                var newDescriptorSize = DependencyPropertyDescriptor.FromProperty(TextBlock.FontSizeProperty, typeof(TextBlock));
                newDescriptorSize?.AddValueChanged(textBlock, OnFontPropertyChanged);
            }
        }

        private static void OnFontPropertyChanged(object? sender, System.EventArgs e)
        {
            if (sender is TextBlock textBlock)
            {
                UpdateInlines(textBlock);
            }
        }

        private static void UpdateInlines(TextBlock textBlock)
        {
            var formattedText = GetFormattedText(textBlock);
            textBlock.Inlines.Clear();

            if (string.IsNullOrEmpty(formattedText)) return;

            const string startTag = "[HL]";
            const string endTag = "[/HL]";
            int lastIndex = 0;

            while (lastIndex < formattedText.Length)
            {
                int startIndex = formattedText.IndexOf(startTag, lastIndex, System.StringComparison.Ordinal);
                if (startIndex == -1)
                {
                    textBlock.Inlines.Add(new Run(formattedText.Substring(lastIndex)));
                    break;
                }

                int endIndex = formattedText.IndexOf(endTag, startIndex + startTag.Length, System.StringComparison.Ordinal);
                if (endIndex == -1)
                {
                    textBlock.Inlines.Add(new Run(formattedText.Substring(lastIndex)));
                    break;
                }

                if (startIndex > lastIndex)
                {
                    textBlock.Inlines.Add(new Run(formattedText.Substring(lastIndex, startIndex - lastIndex)));
                }

                var highlightedText = formattedText.Substring(startIndex + startTag.Length, endIndex - (startIndex + startTag.Length));
                var run = new Run(highlightedText)
                {
                    FontFamily = textBlock.FontFamily,
                    FontSize = textBlock.FontSize,
                    Background = (SolidColorBrush)Application.Current.FindResource("AccentBrush"),
                    Foreground = Brushes.White
                };
                textBlock.Inlines.Add(run);

                lastIndex = endIndex + endTag.Length;
            }
        }
    }
}