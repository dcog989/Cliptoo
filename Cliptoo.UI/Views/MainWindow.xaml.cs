using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;
using Cliptoo.Core;
using Cliptoo.UI.ViewModels;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Views
{
    public partial class MainWindow : FluentWindow
    {
        private readonly MainViewModel _viewModel;
        private readonly CliptooController _controller;
        private readonly List<ClipViewModel> _lastQuickPastedVms = new();

        public MainWindow(MainViewModel viewModel, CliptooController controller)
        {
            InitializeComponent();
            _viewModel = viewModel;
            _controller = controller;
            DataContext = _viewModel;

            _viewModel.IsWindowVisible = IsVisible;
            _viewModel.ListScrolledToTopRequest += OnListScrolledToTopRequest;
            _viewModel.PropertyChanged += ViewModel_PropertyChanged;
        }

        private void ViewModel_PropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
        {
            if (e.PropertyName == nameof(MainViewModel.IsQuickPasteModeActive))
            {
                UpdateQuickPasteIndices();
            }
        }

        private void UpdateQuickPasteIndices()
        {
            foreach (var clip in _lastQuickPastedVms)
            {
                clip.Index = 0;
            }
            _lastQuickPastedVms.Clear();

            if (!_viewModel.IsQuickPasteModeActive) return;

            var scrollViewer = FindVisualChild<ScrollViewer>(ClipListView);
            if (scrollViewer == null) return;

            var firstVisibleIndex = (int)scrollViewer.VerticalOffset;

            for (var i = 0; i < 9; i++)
            {
                var targetIndex = firstVisibleIndex + i;
                if (targetIndex < _viewModel.Clips.Count)
                {
                    var vm = _viewModel.Clips[targetIndex];
                    vm.Index = i + 1;
                    _lastQuickPastedVms.Add(vm);
                }
            }
        }

        private void OnListScrolledToTopRequest(object? sender, EventArgs e)
        {
            Dispatcher.BeginInvoke(DispatcherPriority.ContextIdle, new Action(() =>
            {
                if (ClipListView.Items.Count > 0)
                {
                    ClipListView.ScrollIntoView(ClipListView.Items[0]);
                    ClipListView.SelectedIndex = 0;
                }
            }));
        }

        private void ClipListView_PreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (sender is not System.Windows.Controls.ListView listView) return;

            if (e.Key == Key.Down)
            {
                e.Handled = true;
                if (listView.SelectedIndex < listView.Items.Count - 1)
                {
                    listView.SelectedIndex++;
                    listView.ScrollIntoView(listView.SelectedItem);
                }
            }
            else if (e.Key == Key.Up)
            {
                e.Handled = true;
                if (listView.SelectedIndex > 0)
                {
                    listView.SelectedIndex--;
                    listView.ScrollIntoView(listView.SelectedItem);
                }
            }
        }

        private async void MainWindow_Deactivated(object? sender, EventArgs e)
        {
            await Task.Delay(50);

            if (_viewModel.IsFilterPopupOpen)
            {
                _viewModel.IsFilterPopupOpen = false;
            }

            if (_viewModel.IsPreviewOpen)
            {
                _viewModel.RequestHidePreview();
            }

            if (_viewModel.IsHidingExplicitly)
            {
                return;
            }

            if (Application.Current.Windows.OfType<Window>().Any(x => x != this && x.IsActive))
            {
                return;
            }

            if (!this.IsVisible)
            {
                return;
            }

            if (!_viewModel.IsAlwaysOnTop)
            {
                _viewModel.HideWindow();
            }
        }

        protected override void OnClosed(EventArgs e)
        {
            if (_controller != null)
            {
                var settings = _controller.GetSettings();
                settings.WindowWidth = Math.Round(this.Width);
                settings.WindowHeight = Math.Round(this.Height);
                _controller.SaveSettings(settings);
            }

            base.OnClosed(e);
            if (Application.Current != null)
            {
                Application.Current.Shutdown();
            }
        }

        private void Logo_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
        {
            if (e.ButtonState == MouseButtonState.Pressed)
            {
                this.DragMove();
            }
        }

        private void MenuButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is System.Windows.Controls.Button button)
            {
                if (button.ContextMenu is { } contextMenu)
                {
                    contextMenu.PlacementTarget = button;
                    contextMenu.IsOpen = true;
                }
            }
        }

        private static bool HasVisualAncestor(DependencyObject? descendant, DependencyObject? ancestor)
        {
            if (descendant is not Visual)
            {
                return false;
            }

            if (ancestor == null)
                return false;

            DependencyObject? parent = descendant;
            while (parent != null)
            {
                if (parent == ancestor)
                    return true;
                parent = VisualTreeHelper.GetParent(parent);
            }
            return false;
        }

        private void MainWindow_PreviewMouseDown(object sender, MouseButtonEventArgs e)
        {
            var originalSource = e.OriginalSource as DependencyObject;

            if (HasVisualAncestor(originalSource, FilterButton))
            {
                _viewModel.IsFilterPopupOpen = !_viewModel.IsFilterPopupOpen;
                e.Handled = true;
                return;
            }

            if (_viewModel.IsFilterPopupOpen && !HasVisualAncestor(originalSource, FilterPopup.Child))
            {
                _viewModel.IsFilterPopupOpen = false;
            }
        }

        private void ClipListView_MouseLeftButtonUp(object sender, MouseButtonEventArgs e)
        {
            if (sender is not System.Windows.Controls.ListView listView)
                return;

            var dependencyObject = e.OriginalSource as DependencyObject;
            if (dependencyObject == null)
                return;

            var item = System.Windows.Controls.ItemsControl.ContainerFromElement(listView, dependencyObject) as System.Windows.Controls.ListViewItem;

            if (item != null && item.DataContext is ClipViewModel clipVM)
            {
                listView.SelectedItem = item.DataContext;
                _viewModel.PasteClipCommand.Execute(clipVM);
            }
        }

        private void MainWindow_IsVisibleChanged(object sender, DependencyPropertyChangedEventArgs e)
        {
            Cliptoo.Core.Configuration.LogManager.Log($"DIAG: MainWindow_IsVisibleChanged - NewValue: {e.NewValue}");
            _viewModel.IsWindowVisible = (bool)e.NewValue;

            if (e.NewValue is true)
            {
                _viewModel.HandleWindowShown();
                OnListScrolledToTopRequest(this, EventArgs.Empty);
            }
            else if (e.NewValue is false)
            {
                _viewModel.IsHidingExplicitly = false;
            }
        }

        private void MainWindow_Activated(object? sender, EventArgs e)
        {
            if (Mouse.LeftButton == MouseButtonState.Pressed)
            {
                return;
            }

            if (SearchTextBox.IsKeyboardFocusWithin || ClipListView.IsKeyboardFocusWithin)
            {
                return;
            }
            FocusFirstClipItem();
        }

        private void FocusFirstClipItem()
        {
            if (ClipListView.Items.Count > 0)
            {
                Dispatcher.BeginInvoke(DispatcherPriority.ContextIdle, new Action(() =>
                {
                    if (ClipListView.Items.Count > 0)
                    {
                        ClipListView.SelectedIndex = 0;
                        var firstItem = ClipListView.ItemContainerGenerator.ContainerFromIndex(0) as System.Windows.Controls.ListViewItem;
                        firstItem?.Focus();
                    }
                }));
            }
        }

        private void PreviewPopup_MouseEnter(object sender, MouseEventArgs e)
        {
            _viewModel.RequestShowPreview(null);
        }

        private void PreviewPopup_MouseLeave(object sender, MouseEventArgs e)
        {
            _viewModel.RequestHidePreview();
        }

        private void ClipListView_ScrollChanged(object sender, ScrollChangedEventArgs e)
        {
            if (_viewModel.IsQuickPasteModeActive)
            {
                UpdateQuickPasteIndices();
            }

            // Do not trigger load more on upward scroll or during initial layout churn where change can be 0.
            if (e.VerticalChange <= 0)
            {
                return;
            }

            if (e.ExtentHeight <= e.ViewportHeight)
            {
                return;
            }

            // Load more when 10 items are left to scroll.
            if (e.VerticalOffset + e.ViewportHeight >= e.ExtentHeight - 10)
            {
                _viewModel.LoadMoreClipsCommand.Execute(null);
            }
        }

        private static T? FindVisualChild<T>(DependencyObject? obj) where T : DependencyObject
        {
            if (obj == null) return null;

            for (int i = 0; i < VisualTreeHelper.GetChildrenCount(obj); i++)
            {
                DependencyObject child = VisualTreeHelper.GetChild(obj, i);
                if (child is T dependencyObject)
                    return dependencyObject;

                T? childOfChild = FindVisualChild<T>(child);
                if (childOfChild != null)
                    return childOfChild;
            }
            return null;
        }
    }
}