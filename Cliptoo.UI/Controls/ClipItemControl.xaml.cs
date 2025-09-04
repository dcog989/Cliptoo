using System.Windows;
using System.Windows.Controls;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Controls
{
    public partial class ClipItemControl : UserControl
    {
        public ClipItemControl()
        {
            InitializeComponent();
            Unloaded += OnUnloaded;
        }

        private void OnUnloaded(object sender, RoutedEventArgs e)
        {
            if (DataContext is ClipViewModel vm)
            {
                vm.ReleaseThumbnail();
            }
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