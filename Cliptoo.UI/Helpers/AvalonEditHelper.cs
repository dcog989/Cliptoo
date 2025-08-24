using ICSharpCode.AvalonEdit;
using System.Windows;

namespace Cliptoo.UI.Helpers
{
    public static class AvalonEditHelper
    {
        public static readonly DependencyProperty DocumentTextProperty =
            DependencyProperty.RegisterAttached("DocumentText", typeof(string), typeof(AvalonEditHelper),
                new FrameworkPropertyMetadata(string.Empty, FrameworkPropertyMetadataOptions.BindsTwoWayByDefault, OnDocumentTextChanged));

        public static string GetDocumentText(DependencyObject obj)
        {
            return (string)obj.GetValue(DocumentTextProperty);
        }

        public static void SetDocumentText(DependencyObject obj, string value)
        {
            obj.SetValue(DocumentTextProperty, value);
        }

        private static void OnDocumentTextChanged(DependencyObject obj, DependencyPropertyChangedEventArgs e)
        {
            if (obj is TextEditor textEditor)
            {
                if (textEditor.Document != null)
                {
                    var newText = e.NewValue as string ?? string.Empty;
                    if (textEditor.Document.Text != newText)
                    {
                        textEditor.Document.Text = newText;
                    }
                }

                // Use a flag to avoid attaching the event handler multiple times
                if (!(bool)textEditor.GetValue(IsHandlerAttachedProperty))
                {
                    textEditor.TextChanged += TextEditor_TextChanged;
                    textEditor.Unloaded += TextEditor_Unloaded;
                    textEditor.SetValue(IsHandlerAttachedProperty, true);
                }
            }
        }

        private static void TextEditor_TextChanged(object? sender, System.EventArgs e)
        {
            if (sender is TextEditor editor)
            {
                SetDocumentText(editor, editor.Document.Text);
            }
        }

        private static void TextEditor_Unloaded(object sender, RoutedEventArgs e)
        {
            if (sender is TextEditor textEditor)
            {
                textEditor.TextChanged -= TextEditor_TextChanged;
                textEditor.Unloaded -= TextEditor_Unloaded;
                textEditor.SetValue(IsHandlerAttachedProperty, false);
            }
        }

        private static readonly DependencyProperty IsHandlerAttachedProperty =
            DependencyProperty.RegisterAttached("IsHandlerAttached", typeof(bool), typeof(AvalonEditHelper), new PropertyMetadata(false));
    }
}