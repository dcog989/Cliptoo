using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Controls
{
    public partial class ClipItemControl : UserControl
    {
        private readonly DispatcherTimer _loadTimer;

        public ClipItemControl()
        {
            InitializeComponent();

            _loadTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(125) };
            _loadTimer.Tick += OnLoadTimerTick;

            DataContextChanged += OnDataContextChanged;
            Unloaded += OnUnloaded;
        }

        private void OnDataContextChanged(object sender, DependencyPropertyChangedEventArgs e)
        {
            _loadTimer.Stop();

            if (e.OldValue is ClipViewModel oldVm)
            {
                oldVm.ReleaseThumbnail();
            }

            if (e.NewValue is ClipViewModel newVm)
            {
                // Ensure the recycled control doesn't show the previous item's image
                newVm.ReleaseThumbnail();

                // Delay loading to ensure the user has stopped scrolling on this item
                _loadTimer.Start();
            }
        }

        private void OnLoadTimerTick(object? sender, EventArgs e)
        {
            _loadTimer.Stop();
            if (DataContext is ClipViewModel vm)
            {
                _ = vm.LoadThumbnailAsync();
            }
        }

        private void OnUnloaded(object sender, RoutedEventArgs e)
        {
            _loadTimer.Stop();
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
