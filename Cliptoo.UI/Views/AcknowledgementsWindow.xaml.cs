using System.Diagnostics;
using System.Windows.Navigation;
using Wpf.Ui.Appearance;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Views
{
    public partial class AcknowledgementsWindow : FluentWindow
    {
        public AcknowledgementsWindow()
        {
            InitializeComponent();
            ApplicationThemeManager.Apply(this);
            AddHandler(System.Windows.Documents.Hyperlink.RequestNavigateEvent, new RequestNavigateEventHandler(OnRequestNavigate));
        }

        private void OnRequestNavigate(object sender, RequestNavigateEventArgs e)
        {
            Process.Start(new ProcessStartInfo(e.Uri.AbsoluteUri) { UseShellExecute = true });
            e.Handled = true;
        }
    }
}