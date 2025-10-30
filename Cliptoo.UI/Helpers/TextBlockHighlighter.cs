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

        private static readonly DependencyProperty IsHandlerAttachedProperty =
            DependencyProperty.RegisterAttached("IsHandlerAttached", typeof(bool), typeof(TextBlockHighlighter), new PropertyMetadata(false));

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
            if (d is not TextBlock textBlock) return;

            UpdateInlines(textBlock);

            if (!(bool)textBlock.GetValue(IsHandlerAttachedProperty))
            {
                var descriptorFamily = DependencyPropertyDescriptor.FromProperty(TextBlock.FontFamilyProperty, typeof(TextBlock));
                descriptorFamily?.AddValueChanged(textBlock, OnFontPropertyChanged);

                var descriptorSize = DependencyPropertyDescriptor.FromProperty(TextBlock.FontSizeProperty, typeof(TextBlock));
                descriptorSize?.AddValueChanged(textBlock, OnFontPropertyChanged);

                var descriptorForeground = DependencyPropertyDescriptor.FromProperty(TextBlock.ForegroundProperty, typeof(TextBlock));
                descriptorForeground?.AddValueChanged(textBlock, OnFontPropertyChanged);

                textBlock.Unloaded += TextBlock_Unloaded;
                textBlock.SetValue(IsHandlerAttachedProperty, true);
            }
        }

        private static void TextBlock_Unloaded(object sender, RoutedEventArgs e)
        {
            if (sender is not TextBlock textBlock) return;

            var descriptorFamily = DependencyPropertyDescriptor.FromProperty(TextBlock.FontFamilyProperty, typeof(TextBlock));
            descriptorFamily?.RemoveValueChanged(textBlock, OnFontPropertyChanged);

            var descriptorSize = DependencyPropertyDescriptor.FromProperty(TextBlock.FontSizeProperty, typeof(TextBlock));
            descriptorSize?.RemoveValueChanged(textBlock, OnFontPropertyChanged);

            var descriptorForeground = DependencyPropertyDescriptor.FromProperty(TextBlock.ForegroundProperty, typeof(TextBlock));
            descriptorForeground?.RemoveValueChanged(textBlock, OnFontPropertyChanged);

            textBlock.Unloaded -= TextBlock_Unloaded;
            textBlock.SetValue(IsHandlerAttachedProperty, false);
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

            var lvi = VisualTreeUtils.FindVisualAncestor<System.Windows.Controls.ListViewItem>(textBlock);
            bool isSelected = lvi?.IsSelected ?? false;

            Brush currentForeground = textBlock.Foreground;

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
                var highlightBackground = isSelected
                    ? (SolidColorBrush)Application.Current.FindResource("AccentBrushSelectedHighlight")
                    : (SolidColorBrush)Application.Current.FindResource("AccentBrush");

                var highlightRun = new Run(highlightedText)
                {
                    FontFamily = textBlock.FontFamily,
                    FontSize = textBlock.FontSize,
                    Background = highlightBackground,
                    Foreground = (Brush)Application.Current.FindResource("TextOnAccentFillColorPrimaryBrush")
                };
                textBlock.Inlines.Add(highlightRun);

                lastIndex = endIndex + endTag.Length;
            }
        }

    }
}