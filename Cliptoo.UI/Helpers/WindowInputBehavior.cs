using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Helpers
{
    public static class WindowInputBehavior
    {
        public static readonly DependencyProperty SearchTextBoxProperty =
            DependencyProperty.RegisterAttached("SearchTextBox", typeof(TextBox), typeof(WindowInputBehavior), new PropertyMetadata(null));

        public static TextBox GetSearchTextBox(DependencyObject obj) => (TextBox)obj.GetValue(SearchTextBoxProperty);
        public static void SetSearchTextBox(DependencyObject obj, TextBox value) => obj.SetValue(SearchTextBoxProperty, value);

        public static readonly DependencyProperty ClipListViewProperty =
            DependencyProperty.RegisterAttached("ClipListView", typeof(ListView), typeof(WindowInputBehavior), new PropertyMetadata(null));

        public static ListView GetClipListView(DependencyObject obj) => (ListView)obj.GetValue(ClipListViewProperty);
        public static void SetClipListView(DependencyObject obj, ListView value) => obj.SetValue(ClipListViewProperty, value);

        public static readonly DependencyProperty IsEnabledProperty =
            DependencyProperty.RegisterAttached("IsEnabled", typeof(bool), typeof(WindowInputBehavior), new PropertyMetadata(false, OnIsEnabledChanged));

        public static bool GetIsEnabled(DependencyObject obj) => (bool)obj.GetValue(IsEnabledProperty);
        public static void SetIsEnabled(DependencyObject obj, bool value) => obj.SetValue(IsEnabledProperty, value);

        private static void OnIsEnabledChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is not Window window) return;

            if ((bool)e.NewValue)
            {
                window.KeyUp += OnKeyUp;
                window.PreviewKeyDown += OnPreviewKeyDown;
                window.PreviewTextInput += OnPreviewTextInput;
            }
            else
            {
                window.KeyUp -= OnKeyUp;
                window.PreviewKeyDown -= OnPreviewKeyDown;
                window.PreviewTextInput -= OnPreviewTextInput;
            }
        }

        private static MainViewModel? GetViewModel(Window window) => window.DataContext as MainViewModel;

        private static void OnKeyUp(object sender, KeyEventArgs e)
        {
            if (sender is not Window window) return;
            var viewModel = GetViewModel(window);
            if (viewModel == null || !viewModel.IsQuickPasteModeActive) return;

            var quickPasteModifiers = ParseModifiers(viewModel.CurrentSettings.QuickPasteHotkey);
            var key = e.Key == Key.System ? e.SystemKey : e.Key;

            bool wasConfiguredModifierReleased = false;
            if (quickPasteModifiers.HasFlag(ModifierKeys.Control) && (key == Key.LeftCtrl || key == Key.RightCtrl))
                wasConfiguredModifierReleased = true;
            if (quickPasteModifiers.HasFlag(ModifierKeys.Alt) && (key == Key.LeftAlt || key == Key.RightAlt))
                wasConfiguredModifierReleased = true;
            if (quickPasteModifiers.HasFlag(ModifierKeys.Shift) && (key == Key.LeftShift || key == Key.RightShift))
                wasConfiguredModifierReleased = true;
            if (quickPasteModifiers.HasFlag(ModifierKeys.Windows) && (key == Key.LWin || key == Key.RWin))
                wasConfiguredModifierReleased = true;

            if (wasConfiguredModifierReleased)
            {
                viewModel.IsQuickPasteModeActive = false;
            }
        }

        private static void OnPreviewTextInput(object sender, TextCompositionEventArgs e)
        {
            if (sender is not Window window) return;
            var searchTextBox = GetSearchTextBox(window);
            var focusedElement = Keyboard.FocusedElement as DependencyObject;

            if (searchTextBox == null || searchTextBox.IsKeyboardFocusWithin || focusedElement is TextBox)
            {
                return;
            }

            searchTextBox.Focus();
        }

        private static (Key key, ModifierKeys modifiers) ParseHotkey(string hotkeyString)
        {
            var parts = hotkeyString.Split('+').Select(p => p.Trim().ToUpper()).ToList();
            if (parts.Count == 0) return (Key.None, ModifierKeys.None);

            var keyStr = parts.Last();
            var modifiers = parts.Take(parts.Count - 1);

            ModifierKeys modifierFlags = ModifierKeys.None;
            if (modifiers.Contains("CTRL")) modifierFlags |= ModifierKeys.Control;
            if (modifiers.Contains("ALT")) modifierFlags |= ModifierKeys.Alt;
            if (modifiers.Contains("SHIFT")) modifierFlags |= ModifierKeys.Shift;
            if (modifiers.Contains("WIN")) modifierFlags |= ModifierKeys.Windows;

            if (Enum.TryParse<Key>(keyStr, true, out var key))
            {
                return (key, modifierFlags);
            }

            return (Key.None, ModifierKeys.None);
        }

        private static void OnPreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (sender is not Window window) return;

            var viewModel = GetViewModel(window);
            var searchTextBox = GetSearchTextBox(window);
            var clipListView = GetClipListView(window);
            var focusedElement = Keyboard.FocusedElement as DependencyObject;

            if (viewModel == null || searchTextBox == null || clipListView == null) return;

            // Handle Quick Paste activation
            if (!e.IsRepeat)
            {
                var quickPasteModifiers = ParseModifiers(viewModel.CurrentSettings.QuickPasteHotkey);
                if (Keyboard.Modifiers == quickPasteModifiers && !viewModel.IsQuickPasteModeActive)
                {
                    viewModel.IsQuickPasteModeActive = true;
                }
            }

            if (e.IsRepeat)
            {
                bool isRepeatableKey = e.Key is Key.Up or Key.Down or Key.PageUp or Key.PageDown;
                if (!isRepeatableKey)
                {
                    return;
                }
            }

            if (e.Key == Key.Escape)
            {
                viewModel.HideWindow();
                e.Handled = true;
                return;
            }

            // Handle Preview Hotkey
            var (previewKey, previewModifiers) = ParseHotkey(viewModel.CurrentSettings.PreviewHotkey);
            if (Keyboard.Modifiers == previewModifiers && e.Key == previewKey)
            {
                var selectedItemContainer = clipListView.ItemContainerGenerator.ContainerFromItem(clipListView.SelectedItem) as UIElement;
                viewModel.TogglePreviewForSelection(selectedItemContainer);
                e.Handled = true;
                return;
            }

            if (viewModel.IsPasting)
            {
                e.Handled = true;
                return;
            }

            var quickPastePasteModifiers = ParseModifiers(viewModel.CurrentSettings.QuickPasteHotkey);
            if (Keyboard.Modifiers == quickPastePasteModifiers)
            {
                int number = -1;
                if (e.Key >= Key.D1 && e.Key <= Key.D9) number = e.Key - Key.D1;
                else if (e.Key >= Key.NumPad1 && e.Key <= Key.NumPad9) number = e.Key - Key.NumPad1;

                if (number != -1)
                {
                    var scrollViewer = FindVisualChild<ScrollViewer>(clipListView);
                    var firstVisibleIndex = scrollViewer != null ? (int)scrollViewer.VerticalOffset : 0;
                    var targetIndex = firstVisibleIndex + number;

                    if (targetIndex < viewModel.Clips.Count)
                    {
                        viewModel.PasteClipCommand.Execute(viewModel.Clips[targetIndex]);
                        e.Handled = true;
                        return;
                    }
                }
            }

            if (searchTextBox.IsKeyboardFocusWithin)
            {
                if (e.Key == Key.Enter)
                {
                    e.Handled = true;
                    ClipViewModel? itemToPaste = null;
                    if (clipListView.SelectedIndex > -1 && clipListView.SelectedIndex < viewModel.Clips.Count)
                    {
                        itemToPaste = viewModel.Clips[clipListView.SelectedIndex];
                    }
                    else
                    {
                        itemToPaste = viewModel.Clips.FirstOrDefault();
                    }

                    if (itemToPaste != null)
                    {
                        viewModel.PasteClipCommand.Execute(itemToPaste);
                    }
                    return;
                }

                if (e.Key == Key.Up || e.Key == Key.Down)
                {
                    e.Handled = true;
                    if (clipListView.Items.Count == 0) return;

                    int newIndex = clipListView.SelectedIndex;

                    if (e.Key == Key.Down)
                    {
                        newIndex++;
                    }
                    else // Key.Up
                    {
                        newIndex--;
                    }

                    if (newIndex < 0) newIndex = 0;
                    if (newIndex >= clipListView.Items.Count) newIndex = clipListView.Items.Count - 1;

                    clipListView.SelectedIndex = newIndex;
                    clipListView.ScrollIntoView(clipListView.SelectedItem);

                    return;
                }

                if (e.Key == Key.PageUp || e.Key == Key.PageDown)
                {
                    e.Handled = true;
                    var scrollViewer = FindVisualChild<ScrollViewer>(clipListView);
                    if (scrollViewer != null)
                    {
                        if (e.Key == Key.PageUp)
                        {
                            scrollViewer.PageUp();
                        }
                        else
                        {
                            scrollViewer.PageDown();
                        }
                    }
                    return;
                }

                return;
            }

            if (focusedElement != null && IsDescendantOf(focusedElement, clipListView))
            {
                if (e.Key == Key.Back || e.Key == Key.Delete)
                {
                    e.Handled = true;
                    searchTextBox.Focus();
                    var newEvent = new KeyEventArgs(e.KeyboardDevice, e.InputSource, e.Timestamp, e.Key) { RoutedEvent = Keyboard.KeyDownEvent };
                    searchTextBox.RaiseEvent(newEvent);
                }
                else if (e.Key == Key.Space)
                {
                    e.Handled = true;
                    searchTextBox.Focus();
                    var caretIndex = searchTextBox.CaretIndex;
                    var selectionLength = searchTextBox.SelectionLength;
                    searchTextBox.Text = searchTextBox.Text.Remove(caretIndex, selectionLength).Insert(caretIndex, " ");
                    searchTextBox.CaretIndex = caretIndex + 1;
                }
            }
        }

        private static bool IsDescendantOf(DependencyObject child, DependencyObject parent)
        {
            if (child == null || parent == null) return false;
            var current = child;
            while (current != null)
            {
                if (current == parent) return true;
                current = VisualTreeHelper.GetParent(current);
            }
            return false;
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

        private static ModifierKeys ParseModifiers(string modifiersString)
        {
            var parts = modifiersString.Split('+').Select(p => p.Trim().ToUpper()).ToList();
            ModifierKeys modifierFlags = ModifierKeys.None;
            if (parts.Contains("CTRL")) modifierFlags |= ModifierKeys.Control;
            if (parts.Contains("ALT")) modifierFlags |= ModifierKeys.Alt;
            if (parts.Contains("SHIFT")) modifierFlags |= ModifierKeys.Shift;
            if (parts.Contains("WIN")) modifierFlags |= ModifierKeys.Windows;
            return modifierFlags;
        }

    }
}