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
                    case AppConstants.HotkeyTargetMain: this.Hotkey = string.Empty; break;
                    case AppConstants.HotkeyTargetPreview: this.PreviewHotkey = string.Empty; break;
                    case AppConstants.HotkeyTargetQuickPaste: this.QuickPasteHotkey = string.Empty; break;
                }
                return;
            }

            bool isModifierKey = key is Key.LeftCtrl or Key.RightCtrl or Key.LeftAlt or Key.RightAlt or Key.LeftShift or Key.RightShift or Key.LWin or Key.RWin or Key.None;

            if (target != AppConstants.HotkeyTargetQuickPaste && isModifierKey)
            {
                return;
            }

            var hotkeyParts = new System.Collections.Generic.List<string>();
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Control)) hotkeyParts.Add("Ctrl");
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Alt)) hotkeyParts.Add("Alt");
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Shift)) hotkeyParts.Add("Shift");
            if (Keyboard.Modifiers.HasFlag(ModifierKeys.Windows)) hotkeyParts.Add("Win");

            if (!isModifierKey && target != AppConstants.HotkeyTargetQuickPaste)
            {
                hotkeyParts.Add(key.ToString());
            }

            var newHotkey = string.Join("+", hotkeyParts);

            switch (target)
            {
                case AppConstants.HotkeyTargetMain:
                    this.Hotkey = newHotkey;
                    break;
                case AppConstants.HotkeyTargetPreview:
                    this.PreviewHotkey = newHotkey;
                    break;
                case AppConstants.HotkeyTargetQuickPaste:
                    this.QuickPasteHotkey = newHotkey;
                    break;
            }
        }

        private async Task ShowInvalidHotkeyDialog(string hotkeyName, string reason, string defaultValue)
        {
            var dialog = new ContentDialog
            {
                Title = "Invalid Hotkey",
                Content = $"The {hotkeyName} is invalid. It {reason}. It has been reset to the default '{defaultValue}'.",
                CloseButtonText = "OK"
            };
            await _contentDialogService.ShowAsync(dialog, CancellationToken.None).ConfigureAwait(false);
        }

        public async Task ValidateHotkey(string target)
        {
            string[] parts;
            bool isValid;

            switch (target)
            {
                case AppConstants.HotkeyTargetMain:
                    parts = this.Hotkey.Split(_plusSeparator, StringSplitOptions.RemoveEmptyEntries);
                    var mainKeyPart = parts.LastOrDefault();
                    // Must have at least one modifier and one non-modifier key
                    isValid = parts.Length >= 2 && mainKeyPart is not "Ctrl" and not "Alt" and not "Shift" and not "Win";
                    if (!isValid)
                    {
                        this.Hotkey = "Ctrl+Alt+V";
                        await ShowInvalidHotkeyDialog("Launch main window hotkey", "must include at least one modifier (e.g., Ctrl, Alt) and a regular key (e.g., V)", this.Hotkey);
                    }
                    break;

                case AppConstants.HotkeyTargetPreview:
                    parts = this.PreviewHotkey.Split(_plusSeparator, StringSplitOptions.RemoveEmptyEntries);
                    var previewKeyPart = parts.LastOrDefault();
                    // Must have at least one non-modifier key
                    isValid = parts.Length >= 1 && previewKeyPart is not "Ctrl" and not "Alt" and not "Shift" and not "Win";
                    if (!isValid)
                    {
                        this.PreviewHotkey = "F3";
                        await ShowInvalidHotkeyDialog("Preview tooltip hotkey", "must include a regular key (e.g., F3)", this.PreviewHotkey);
                    }
                    break;

                case AppConstants.HotkeyTargetQuickPaste:
                    parts = this.QuickPasteHotkey.Split(_plusSeparator, StringSplitOptions.RemoveEmptyEntries);
                    bool allModifiers = parts.All(p => p is "Ctrl" or "Alt" or "Shift" or "Win");
                    isValid = allModifiers && parts.Length >= 2;
                    if (!isValid)
                    {
                        this.QuickPasteHotkey = "Ctrl+Alt";
                        await ShowInvalidHotkeyDialog("Quick Paste hotkey", "must consist of at least two modifier keys (e.g., Ctrl, Alt, Shift)", this.QuickPasteHotkey);
                    }
                    break;
            }
            _settingsService.SaveSettings();
        }

    }
}