using System.Windows.Input;
using Cliptoo.Core;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.ViewModels
{
    internal partial class SettingsViewModel
    {
        private bool _isCapturingHotkey;
        public bool IsCapturingHotkey { get => _isCapturingHotkey; set => SetProperty(ref _isCapturingHotkey, value); }
        private string? _capturingHotkeyTarget;
        public string? CapturingHotkeyTarget { get => _capturingHotkeyTarget; set => SetProperty(ref _capturingHotkeyTarget, value); }
        private static readonly char[] _plusSeparator = ['+'];

        public void UpdateHotkey(KeyEventArgs e, string target)
        {
            ArgumentNullException.ThrowIfNull(e);
            if (!IsCapturingHotkey || CapturingHotkeyTarget != target) return;
            e.Handled = true;

            var key = (e.Key == Key.System) ? e.SystemKey : e.Key;

            if (key is Key.Back or Key.Delete)
            {
                switch (target)
                {
                    case AppConstants.HotkeyTargets.Main: this.Hotkey = string.Empty; break;
                    case AppConstants.HotkeyTargets.Preview: this.PreviewHotkey = string.Empty; break;
                    case AppConstants.HotkeyTargets.QuickPaste: this.QuickPasteHotkey = string.Empty; break;
                }
                return;
            }

            bool isModifierKey = key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin or Key.None;

            if (target != AppConstants.HotkeyTargets.QuickPaste && isModifierKey)
            {
                return;
            }

            var hotkeyParts = new System.Collections.Generic.List<string>();
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Control)) hotkeyParts.Add("Ctrl");
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Alt)) hotkeyParts.Add("Alt");
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Shift)) hotkeyParts.Add("Shift");
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Windows)) hotkeyParts.Add("Win");

            if (!isModifierKey && target != AppConstants.HotkeyTargets.QuickPaste)
            {
                hotkeyParts.Add(key.ToString());
            }

            var newHotkey = string.Join("+", hotkeyParts);

            switch (target)
            {
                case AppConstants.HotkeyTargets.Main:
                    this.Hotkey = newHotkey;
                    break;
                case AppConstants.HotkeyTargets.Preview:
                    this.PreviewHotkey = newHotkey;
                    break;
                case AppConstants.HotkeyTargets.QuickPaste:
                    this.QuickPasteHotkey = newHotkey;
                    break;
            }
        }

        public async Task ValidateHotkey(string target)
        {
            if (target == AppConstants.HotkeyTargets.QuickPaste)
            {
                var parts = this.QuickPasteHotkey.Split(_plusSeparator, StringSplitOptions.RemoveEmptyEntries);
                bool allModifiers = parts.All(p => p is "Ctrl" or "Alt" or "Shift" or "Win");

                if (!allModifiers || parts.Length < 2)
                {
                    this.QuickPasteHotkey = "Ctrl+Alt";
                    var dialog = new ContentDialog
                    {
                        Title = "Invalid Hotkey",
                        Content = "The Quick Paste hotkey must consist of at least two modifier keys (e.g., Ctrl, Alt, Shift). It has been reset to the default 'Ctrl+Alt'.",
                        CloseButtonText = "OK"
                    };
                    await _contentDialogService.ShowAsync(dialog, CancellationToken.None).ConfigureAwait(false);
                }
            }
        }

    }
}