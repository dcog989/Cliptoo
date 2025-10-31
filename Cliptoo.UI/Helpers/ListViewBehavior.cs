using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;

namespace Cliptoo.UI.Helpers
{
    public static class ListViewBehavior
    {
        // IsEnabled Attached Property (to activate the behavior)
        public static readonly DependencyProperty IsEnabledProperty =
            DependencyProperty.RegisterAttached("IsEnabled", typeof(bool), typeof(ListViewBehavior), new PropertyMetadata(false, OnIsEnabledChanged));

        public static bool GetIsEnabled(DependencyObject obj) => (bool)obj.GetValue(IsEnabledProperty);
        public static void SetIsEnabled(DependencyObject obj, bool value) => obj.SetValue(IsEnabledProperty, value);

        // LoadMoreCommand Attached Property
        public static readonly DependencyProperty LoadMoreCommandProperty =
            DependencyProperty.RegisterAttached("LoadMoreCommand", typeof(ICommand), typeof(ListViewBehavior), new PropertyMetadata(null));

        public static ICommand? GetLoadMoreCommand(DependencyObject obj) => (ICommand?)obj.GetValue(LoadMoreCommandProperty);
        public static void SetLoadMoreCommand(DependencyObject obj, ICommand? value) => obj.SetValue(LoadMoreCommandProperty, value);

        // RequestScrollToTop Attached Property
        public static readonly DependencyProperty RequestScrollToTopProperty =
            DependencyProperty.RegisterAttached("RequestScrollToTop", typeof(bool), typeof(ListViewBehavior), new PropertyMetadata(false, OnRequestScrollToTopChanged));

        public static bool GetRequestScrollToTop(DependencyObject obj) => (bool)obj.GetValue(RequestScrollToTopProperty);
        public static void SetRequestScrollToTop(DependencyObject obj, bool value) => obj.SetValue(RequestScrollToTopProperty, value);

        private static void OnIsEnabledChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is not ListView listView) return;

            if ((bool)e.NewValue)
            {
                listView.Loaded += OnListViewLoaded;
                listView.Unloaded += OnListViewUnloaded;
                listView.PreviewKeyDown += OnListViewPreviewKeyDown;
            }
            else
            {
                listView.Loaded -= OnListViewLoaded;
                listView.Unloaded -= OnListViewUnloaded;
                listView.PreviewKeyDown -= OnListViewPreviewKeyDown;
                var scrollViewer = VisualTreeUtils.FindVisualChild<ScrollViewer>(listView);
                if (scrollViewer != null)
                {
                    scrollViewer.ScrollChanged -= OnScrollViewerScrollChanged;
                }
            }
        }

        private static void OnListViewLoaded(object sender, RoutedEventArgs e)
        {
            if (sender is ListView listView)
            {
                var scrollViewer = VisualTreeUtils.FindVisualChild<ScrollViewer>(listView);
                if (scrollViewer != null)
                {
                    scrollViewer.ScrollChanged += OnScrollViewerScrollChanged;
                }
            }
        }

        private static void OnListViewUnloaded(object sender, RoutedEventArgs e)
        {
            if (sender is ListView listView)
            {
                var scrollViewer = VisualTreeUtils.FindVisualChild<ScrollViewer>(listView);
                if (scrollViewer != null)
                {
                    scrollViewer.ScrollChanged -= OnScrollViewerScrollChanged;
                }
            }
        }

        private static void OnScrollViewerScrollChanged(object sender, ScrollChangedEventArgs e)
        {
            if (sender is not ScrollViewer scrollViewer || e.VerticalChange <= 0 || e.ExtentHeight <= e.ViewportHeight) return;

            // Trigger load more when scroll is near the bottom
            if (e.VerticalOffset + e.ViewportHeight >= e.ExtentHeight - 10)
            {
                var listView = VisualTreeUtils.FindVisualAncestor<ListView>(scrollViewer);
                if (listView != null)
                {
                    var command = GetLoadMoreCommand(listView);
                    if (command != null && command.CanExecute(null))
                    {
                        command.Execute(null);
                    }
                }
            }
        }

        private static void OnListViewPreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (sender is not ListView listView || listView.Items.Count == 0) return;

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

        private static void OnRequestScrollToTopChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is ListView listView && (bool)e.NewValue)
            {
                listView.Dispatcher.BeginInvoke(System.Windows.Threading.DispatcherPriority.ContextIdle, new Action(() =>
                {
                    if (listView.Items.Count > 0)
                    {
                        listView.ScrollIntoView(listView.Items[0]);
                        listView.SelectedIndex = 0;

                        var window = Window.GetWindow(listView);
                        var searchTextBox = window != null ? WindowInputBehavior.GetSearchTextBox(window) : null;

                        if (searchTextBox == null || !searchTextBox.IsKeyboardFocusWithin)
                        {
                            var firstItem = listView.ItemContainerGenerator.ContainerFromIndex(0) as ListViewItem;
                            firstItem?.Focus();
                        }
                    }
                    // Reset the trigger
                    SetRequestScrollToTop(listView, false);
                }));
            }
        }
    }
}