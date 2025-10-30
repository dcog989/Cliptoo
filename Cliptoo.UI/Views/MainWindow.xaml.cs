using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;
using Cliptoo.Core.Interfaces;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.Services;
using Cliptoo.UI.ViewModels;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Views
{
    public partial class MainWindow : FluentWindow
    {
        private readonly MainViewModel _viewModel;
        private readonly ISettingsService _settingsService;
        private readonly IListViewInteractionService _listViewInteractionService;
        private readonly DispatcherTimer _saveStateDebounceTimer;

        public MainWindow(MainViewModel viewModel, ISettingsService settingsService, IListViewInteractionService listViewInteractionService)
        {
            InitializeComponent();
            _viewModel = viewModel;
            _settingsService = settingsService;
            _listViewInteractionService = listViewInteractionService;
            DataContext = _viewModel;

            _viewModel.IsWindowVisible = IsVisible;
            _viewModel.ListScrolledToTopRequest += OnListScrolledToTopRequest;

            _saveStateDebounceTimer = new DispatcherTimer
            {
                Interval = TimeSpan.FromMilliseconds(500)
            };
            _saveStateDebounceTimer.Tick += SaveWindowState_Tick;
            SizeChanged += OnWindowResizedOrMoved;
            LocationChanged += OnWindowResizedOrMoved;
        }

        public SnackbarPresenter SnackbarPresenter => RootSnackbarPresenter;

        private void OnWindowResizedOrMoved(object? sender, EventArgs e)
        {
            _saveStateDebounceTimer.Stop();
            _saveStateDebounceTimer.Start();
        }

        private void SaveWindowState_Tick(object? sender, EventArgs e)
        {
            _saveStateDebounceTimer.Stop();
            if (this.WindowState == WindowState.Normal)
            {
                var settings = _settingsService.Settings;
                settings.WindowWidth = Math.Round(this.Width);
                settings.WindowHeight = Math.Round(this.Height);
                settings.FixedX = (int)Math.Round(this.Left);
                settings.FixedY = (int)Math.Round(this.Top);
                _settingsService.SaveSettings();
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
                var ancestor = VisualTreeUtils.FindVisualAncestor<System.Windows.Controls.ListViewItem>(hitTestResult.VisualHit);
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
                var ancestor = VisualTreeUtils.FindVisualAncestor<System.Windows.Controls.ListViewItem>(hitTestResult.VisualHit);
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
            SizeChanged -= OnWindowResizedOrMoved;
            LocationChanged -= OnWindowResizedOrMoved;
            if (_saveStateDebounceTimer != null)
            {
                _saveStateDebounceTimer.Tick -= SaveWindowState_Tick;
            }

            _viewModel.ListScrolledToTopRequest -= OnListScrolledToTopRequest;

            // Save final state on graceful close, in case a change was made within the debounce interval.
            SaveWindowState_Tick(null, EventArgs.Empty);

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

        private void MainWindow_PreviewMouseDown(object sender, MouseButtonEventArgs e)
        {
            var originalSource = e.OriginalSource as DependencyObject;

            if (VisualTreeUtils.HasVisualAncestor(originalSource, FilterButton))
            {
                _viewModel.IsFilterPopupOpen = !_viewModel.IsFilterPopupOpen;
                e.Handled = true;
                return;
            }

            if (_viewModel.IsFilterPopupOpen && !VisualTreeUtils.HasVisualAncestor(originalSource, FilterPopup.Child))
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
            _listViewInteractionService.FirstVisibleIndex = (int)e.VerticalOffset;
            if (_viewModel.IsQuickPasteModeActive)
            {
                _viewModel.UpdateQuickPasteIndices();
            }

            if (e.VerticalChange != 0 && _viewModel.PreviewManager.IsPreviewOpen)
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

    }
}