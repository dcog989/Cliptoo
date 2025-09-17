using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;
using Cliptoo.Core.Interfaces;
using Cliptoo.UI.ViewModels;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Views
{
    public partial class MainWindow : FluentWindow
    {
        private readonly MainViewModel _viewModel;
        private readonly ISettingsService _settingsService;

        public MainWindow(MainViewModel viewModel, ISettingsService settingsService)
        {
            InitializeComponent();
            _viewModel = viewModel;
            _settingsService = settingsService;
            DataContext = _viewModel;

            _viewModel.IsWindowVisible = IsVisible;
            _viewModel.ListScrolledToTopRequest += OnListScrolledToTopRequest;
        }

        public SnackbarPresenter SnackbarPresenter => RootSnackbarPresenter;

        private void OnListScrolledToTopRequest(object? sender, EventArgs e)
        {
            Dispatcher.BeginInvoke(DispatcherPriority.ContextIdle, new Action(() =>
            {
                if (ClipListView.Items.Count > 0)
                {
                    ClipListView.ScrollIntoView(ClipListView.Items[0]);
                    ClipListView.SelectedIndex = 0;
                    if (!SearchTextBox.IsKeyboardFocusWithin)
                    {
                        var firstItem = ClipListView.ItemContainerGenerator.ContainerFromIndex(0) as System.Windows.Controls.ListViewItem;
                        firstItem?.Focus();
                    }
                }
            }));
        }

        private void ClipListView_PreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (sender is not System.Windows.Controls.ListView listView || listView.Items.Count == 0) return;

            switch (e.Key)
            {
                case Key.Down:
                    e.Handled = true;
                    if (listView.SelectedIndex < listView.Items.Count - 1)
                    {
                        listView.SelectedIndex++;
                        listView.ScrollIntoView(listView.SelectedItem);
                    }
                    break;
                case Key.Up:
                    e.Handled = true;
                    if (listView.SelectedIndex > 0)
                    {
                        listView.SelectedIndex--;
                        listView.ScrollIntoView(listView.SelectedItem);
                    }
                    break;
                case Key.Home:
                    e.Handled = true;
                    listView.SelectedIndex = 0;
                    listView.ScrollIntoView(listView.SelectedItem);
                    break;
                case Key.End:
                    e.Handled = true;
                    listView.SelectedIndex = listView.Items.Count - 1;
                    listView.ScrollIntoView(listView.SelectedItem);
                    break;
            }
        }

        private void ClipListView_PreviewMouseRightButtonDown(object sender, MouseButtonEventArgs e)
        {
            if (sender is not System.Windows.Controls.ListView listView) return;

            var hitTestResult = VisualTreeHelper.HitTest(listView, e.GetPosition(listView));
            if (hitTestResult?.VisualHit != null)
            {
                var ancestor = FindVisualAncestor<System.Windows.Controls.ListViewItem>(hitTestResult.VisualHit);
                if (ancestor != null)
                {
                    listView.SelectedItem = ancestor.DataContext;
                }
            }
        }

        private void ClipListView_PreviewMouseDown(object sender, MouseButtonEventArgs e)
        {
            // This handler is for clearing selection when clicking on an empty area.
            var hitTestResult = VisualTreeHelper.HitTest(sender as Visual, e.GetPosition(sender as IInputElement));
            if (hitTestResult != null)
            {
                var ancestor = FindVisualAncestor<System.Windows.Controls.ListViewItem>(hitTestResult.VisualHit);
                if (ancestor == null)
                {
                    // Click was not on an item, so clear selection.
                    if (sender is System.Windows.Controls.ListView lv)
                    {
                        lv.SelectedItem = null;
                    }
                }
            }
        }

        private void MainWindow_Deactivated(object? sender, EventArgs e)
        {
            _viewModel.HandleWindowDeactivated();
        }

        protected override void OnClosed(EventArgs e)
        {
            _viewModel.ListScrolledToTopRequest -= OnListScrolledToTopRequest;
            if (_settingsService != null)
            {
                var settings = _settingsService.Settings;
                settings.WindowWidth = Math.Round(this.Width);
                settings.WindowHeight = Math.Round(this.Height);
                _settingsService.SaveSettings();
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
            var dependencyObject = e.OriginalSource as DependencyObject;
            while (dependencyObject != null)
            {
                if (dependencyObject is FrameworkElement { DataContext: ClipViewModel clipVM })
                {
                    if (sender is System.Windows.Controls.ListView listView)
                    {
                        listView.SelectedItem = clipVM;
                    }
                    _viewModel.PasteClipCommand.Execute(clipVM);
                    e.Handled = true;
                    return;
                }

                if (dependencyObject is Visual || dependencyObject is System.Windows.Media.Media3D.Visual3D)
                {
                    dependencyObject = VisualTreeHelper.GetParent(dependencyObject);
                }
                else if (dependencyObject is FrameworkContentElement fce)
                {
                    dependencyObject = fce.Parent;
                }
                else
                {
                    break;
                }
            }
        }

        private void MainWindow_IsVisibleChanged(object sender, DependencyPropertyChangedEventArgs e)
        {
            _viewModel.IsWindowVisible = (bool)e.NewValue;

            if (e.NewValue is true)
            {
                _viewModel.HandleWindowShown();
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

        private void ClipListView_MouseLeave(object sender, MouseEventArgs e)
        {
            _viewModel.RequestHidePreview();
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
            _viewModel.VerticalScrollOffset = e.VerticalOffset;

            if (e.VerticalChange != 0 && _viewModel.IsPreviewOpen)
            {
                _viewModel.RequestHidePreview();
            }

            if (e.VerticalChange <= 0)
            {
                return;
            }

            if (e.ExtentHeight <= e.ViewportHeight)
            {
                return;
            }

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

        private static T? FindVisualAncestor<T>(DependencyObject? d) where T : DependencyObject
        {
            while (d != null)
            {
                if (d is T target) { return target; }
                if (d is Visual || d is System.Windows.Media.Media3D.Visual3D)
                {
                    d = VisualTreeHelper.GetParent(d);
                }
                else
                {
                    d = LogicalTreeHelper.GetParent(d);
                }
            }
            return null;
        }
    }
}