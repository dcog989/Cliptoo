using System.Windows;
using System.Windows.Input;
using Cliptoo.UI.ViewModels;

namespace Cliptoo.UI.Helpers
{
    internal static class HotkeyInputBehavior
    {
        public static readonly DependencyProperty IsEnabledProperty =
            DependencyProperty.RegisterAttached("IsEnabled", typeof(bool), typeof(HotkeyInputBehavior), new PropertyMetadata(false, OnIsEnabledChanged));

        public static bool GetIsEnabled(DependencyObject obj)
        {
            ArgumentNullException.ThrowIfNull(obj);
            return (bool)obj.GetValue(IsEnabledProperty);
        }

        public static void SetIsEnabled(DependencyObject obj, bool value)
        {
            ArgumentNullException.ThrowIfNull(obj);
            obj.SetValue(IsEnabledProperty, value);
        }

        private static void OnIsEnabledChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is not Wpf.Ui.Controls.TextBox textBox) return;

            if ((bool)e.NewValue)
            {
                textBox.GotFocus += TextBox_GotFocus;
                textBox.LostFocus += TextBox_LostFocus;
                textBox.PreviewKeyDown += TextBox_PreviewKeyDown;
                textBox.PreviewTextInput += TextBox_PreviewTextInput;
            }
            else
            {
                textBox.GotFocus -= TextBox_GotFocus;
                textBox.LostFocus -= TextBox_LostFocus;
                textBox.PreviewKeyDown -= TextBox_PreviewKeyDown;
                textBox.PreviewTextInput -= TextBox_PreviewTextInput;
            }
        }

        public static readonly DependencyProperty HotkeyTargetProperty =
    DependencyProperty.RegisterAttached("HotkeyTarget", typeof(string), typeof(HotkeyInputBehavior), new PropertyMetadata(""));

        public static string GetHotkeyTarget(DependencyObject obj)
        {
            ArgumentNullException.ThrowIfNull(obj);
            return (string)obj.GetValue(HotkeyTargetProperty);
        }

        public static void SetHotkeyTarget(DependencyObject obj, string value)
        {
            ArgumentNullException.ThrowIfNull(obj);
            obj.SetValue(HotkeyTargetProperty, value);
        }

        private static void TextBox_GotFocus(object sender, RoutedEventArgs e)
        {
            if (sender is not Wpf.Ui.Controls.TextBox textBox || textBox.DataContext is not SettingsViewModel vm) return;

            vm.IsCapturingHotkey = true;
            vm.CapturingHotkeyTarget = GetHotkeyTarget(textBox);
        }

        private static void TextBox_LostFocus(object sender, RoutedEventArgs e)
        {
            if (sender is not Wpf.Ui.Controls.TextBox textBox || textBox.DataContext is not SettingsViewModel vm) return;

            vm.ValidateHotkey(GetHotkeyTarget(textBox));

            vm.IsCapturingHotkey = false;
            vm.CapturingHotkeyTarget = null;
        }

        private static void TextBox_PreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (sender is not Wpf.Ui.Controls.TextBox textBox || textBox.DataContext is not SettingsViewModel vm) return;

            vm.UpdateHotkey(e, GetHotkeyTarget(textBox));
        }

        private static void TextBox_PreviewTextInput(object sender, TextCompositionEventArgs e)
        {
            e.Handled = true;
        }
    }
}