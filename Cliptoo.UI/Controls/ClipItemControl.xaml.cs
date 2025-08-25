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

        private void ClipItem_MouseEnter(object sender, MouseEventArgs e)
        {
            if (DataContext is ClipViewModel clipVm)
            {
                clipVm.MainViewModel?.RequestShowPreview(clipVm);
            }
        }

        private void ClipItem_MouseLeave(object sender, MouseEventArgs e)
        {
            if (DataContext is ClipViewModel clipVm)
            {
                clipVm.MainViewModel?.RequestHidePreview();
            }
        }
    }
}