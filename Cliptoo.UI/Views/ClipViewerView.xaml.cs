using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using Cliptoo.UI.ViewModels;
using Wpf.Ui.Appearance;

namespace Cliptoo.UI.Views
{
    public partial class ClipViewerView : UserControl
    {
        public ClipViewerView()
        {
            InitializeComponent();
            DataContextChanged += OnDataContextChanged;
            Loaded += OnLoaded;
        }

        private void OnLoaded(object sender, RoutedEventArgs e)
        {
            UpdateLinkColor();
        }

        private void OnDataContextChanged(object sender, DependencyPropertyChangedEventArgs e)
        {
            UpdateLinkColor();
        }

        private void UpdateLinkColor()
        {
            var theme = ApplicationThemeManager.GetAppTheme();
            if (theme == ApplicationTheme.Unknown)
            {
                theme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
            }

            if (theme == ApplicationTheme.Dark)
            {
                var brush = FindResource("HyperlinkBlueBrush") as Brush;
                TextEditor.TextArea.TextView.LinkTextForegroundBrush = brush ?? Brushes.CornflowerBlue;
            }
            else
            {
                // Use the default AvalonEdit brush for light mode.
                TextEditor.TextArea.TextView.LinkTextForegroundBrush = null;
            }
        }
    }
}