using System.Windows;
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
            DataContextChanged += OnDataContextChanged;
            Unloaded += OnUnloaded;
        }

        private void OnDataContextChanged(object sender, DependencyPropertyChangedEventArgs e)
        {
            if (e.OldValue is ClipViewModel oldVm)
            {
                oldVm.ReleaseThumbnail();
            }
            if (e.NewValue is ClipViewModel newVm)
            {
                _ = newVm.LoadThumbnailAsync();
            }
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
                clipVm.RequestShowPreview();
            }
            e.Handled = true;
        }

        private void ClipItem_MouseLeave(object sender, MouseEventArgs e)
        {
            if (DataContext is ClipViewModel clipVm)
            {
                clipVm.RequestHidePreview();
            }
        }

    }
}