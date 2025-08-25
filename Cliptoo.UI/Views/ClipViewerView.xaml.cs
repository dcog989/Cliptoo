using System.Windows;
using System.Windows.Controls;
using System.Xml;
using Cliptoo.Core.Services;
using Cliptoo.UI.ViewModels;
using ICSharpCode.AvalonEdit.Highlighting;
using ICSharpCode.AvalonEdit.Highlighting.Xshd;
using Microsoft.Extensions.DependencyInjection;
using Wpf.Ui.Appearance;

namespace Cliptoo.UI.Views
{
    public partial class ClipViewerView : UserControl
    {
        public ClipViewerView()
        {
            InitializeComponent();
            this.Loaded += OnLoaded;
        }

        private async void OnLoaded(object sender, RoutedEventArgs e)
        {
            if (DataContext is not ClipViewerViewModel vm) return;

            try
            {
                var clip = await vm.Controller.GetClipByIdAsync(vm.ClipId);
                if (clip == null) return;

                var syntaxHighlighter = App.Services.GetRequiredService<ISyntaxHighlighter>();
                var definitionName = syntaxHighlighter.GetHighlightingDefinition(clip.ClipType, clip.Content ?? string.Empty);

                if (definitionName != null)
                {
                    var theme = ApplicationThemeManager.GetAppTheme();
                    if (theme == ApplicationTheme.Unknown)
                    {
                        theme = ApplicationThemeManager.GetSystemTheme() == SystemTheme.Dark ? ApplicationTheme.Dark : ApplicationTheme.Light;
                    }

                    IHighlightingDefinition? highlighting = null;
                    if (theme == ApplicationTheme.Dark)
                    {
                        TextEditor.Foreground = System.Windows.Media.Brushes.Gainsboro;
                        if (definitionName == "C#")
                        {
                            var uri = new Uri("pack://application:,,,/Assets/AvalonEditThemes/CSharp-Dark.xshd");
                            var resourceInfo = Application.GetResourceStream(uri);
                            if (resourceInfo != null)
                            {
                                using (var stream = resourceInfo.Stream)
                                {
                                    using (var reader = new XmlTextReader(stream))
                                    {
                                        highlighting = HighlightingLoader.Load(reader, HighlightingManager.Instance);
                                    }
                                }
                            }
                        }
                    }

                    TextEditor.SyntaxHighlighting = highlighting ?? HighlightingManager.Instance.GetDefinition(definitionName);
                }
            }
            catch (Exception ex)
            {
                Core.Configuration.LogManager.Log(ex, "Failed to load syntax highlighting in ClipViewerView.");
            }
        }
    }
}