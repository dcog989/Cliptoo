using System.Windows.Controls;
using System.Windows.Input;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Controls
{
    public partial class ClipItemControl : UserControl
    {
        public ClipItemControl()
        {
            InitializeComponent();
        }

        private void ClipItem_ToolTipOpening(object sender, ToolTipEventArgs e)
        {
            if (DataContext is ClipViewModel clipVm)
            {
                clipVm.MainViewModel?.RequestShowPreview(clipVm);
            }
            e.Handled = true;
        }
    }
}