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

                var oldDescriptorForeground = DependencyPropertyDescriptor.FromProperty(TextBlock.ForegroundProperty, typeof(TextBlock));
                oldDescriptorForeground?.RemoveValueChanged(textBlock, OnFontPropertyChanged);

                UpdateInlines(textBlock);

                // Subscribe to future property changes
                var newDescriptorFamily = DependencyPropertyDescriptor.FromProperty(TextBlock.FontFamilyProperty, typeof(TextBlock));
                newDescriptorFamily?.AddValueChanged(textBlock, OnFontPropertyChanged);

                var newDescriptorSize = DependencyPropertyDescriptor.FromProperty(TextBlock.FontSizeProperty, typeof(TextBlock));
                newDescriptorSize?.AddValueChanged(textBlock, OnFontPropertyChanged);

                var newDescriptorForeground = DependencyPropertyDescriptor.FromProperty(TextBlock.ForegroundProperty, typeof(TextBlock));
                newDescriptorForeground?.AddValueChanged(textBlock, OnFontPropertyChanged);
            }
        }

        private static void OnFontPropertyChanged(object? sender, System.EventArgs e)
        {
            if (sender is TextBlock textBlock)
            {
                var lvi = FindAncestor<System.Windows.Controls.ListViewItem>(textBlock);
                var lviForeground = lvi?.Foreground;
                UpdateInlines(textBlock);
            }
        }

        private static void UpdateInlines(TextBlock textBlock)
        {
            var formattedText = GetFormattedText(textBlock);
            textBlock.Inlines.Clear();

            if (string.IsNullOrEmpty(formattedText)) return;

            var lvi = FindAncestor<System.Windows.Controls.ListViewItem>(textBlock);
            bool isSelected = lvi?.IsSelected ?? false;

            Brush currentForeground = isSelected ? Brushes.White : textBlock.Foreground;

            const string startTag = "[HL]";
            const string endTag = "[/HL]";
            int lastIndex = 0;

            while (lastIndex < formattedText.Length)
            {
                int startIndex = formattedText.IndexOf(startTag, lastIndex, System.StringComparison.Ordinal);
                if (startIndex == -1)
                {
                    var run = new Run(formattedText.Substring(lastIndex)) { Foreground = currentForeground };
                    textBlock.Inlines.Add(run);
                    break;
                }

                int endIndex = formattedText.IndexOf(endTag, startIndex + startTag.Length, System.StringComparison.Ordinal);
                if (endIndex == -1)
                {
                    var run = new Run(formattedText.Substring(lastIndex)) { Foreground = currentForeground };
                    textBlock.Inlines.Add(run);
                    break;
                }

                if (startIndex > lastIndex)
                {
                    var run = new Run(formattedText.Substring(lastIndex, startIndex - lastIndex)) { Foreground = currentForeground };
                    textBlock.Inlines.Add(run);
                }

                var highlightedText = formattedText.Substring(startIndex + startTag.Length, endIndex - (startIndex + startTag.Length));
                var highlightRun = new Run(highlightedText)
                {
                    FontFamily = textBlock.FontFamily,
                    FontSize = textBlock.FontSize,
                    Background = (SolidColorBrush)Application.Current.FindResource("AccentBrush"),
                    Foreground = Brushes.White
                };
                textBlock.Inlines.Add(highlightRun);

                lastIndex = endIndex + endTag.Length;
            }
        }

        private static T? FindAncestor<T>(DependencyObject current) where T : DependencyObject
        {
            do
            {
                if (current is T ancestor)
                {
                    return ancestor;
                }
                current = VisualTreeHelper.GetParent(current);
            }
            while (current != null);
            return null;
        }
    }
}