using System;
using System.Linq;
using System.Runtime.InteropServices;
using System.Windows.Input;

namespace Cliptoo.Core.Native
{
    public class GlobalHotkey : IGlobalHotkey
    {
        public event Action? HotkeyPressed;

        private IntPtr _windowHandle;
        private const int HotkeyId = 9000;

        // Win32 API
        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        private static extern bool UnregisterHotKey(IntPtr hWnd, int id);

        [Flags]
        private enum ModifierKeys : uint
        {
            Alt = 1,
            Control = 2,
            Shift = 4,
            Win = 8
        }

        public GlobalHotkey(IntPtr windowHandle)
        {
            _windowHandle = windowHandle;
        }

        public bool Register(string hotkey)
        {
            Unregister(); // Ensure any previous hotkey is unregistered

            try
            {
                var parts = hotkey.Split('+').Select(p => p.Trim().ToUpper()).ToList();
                if (parts.Count < 2) return false;

                var keyStr = parts.Last();
                var modifiers = parts.Take(parts.Count - 1);

                uint modifierFlags = 0;
                if (modifiers.Contains("CTRL")) modifierFlags |= (uint)ModifierKeys.Control;
                if (modifiers.Contains("ALT")) modifierFlags |= (uint)ModifierKeys.Alt;
                if (modifiers.Contains("SHIFT")) modifierFlags |= (uint)ModifierKeys.Shift;
                if (modifiers.Contains("WIN")) modifierFlags |= (uint)ModifierKeys.Win;

                var key = Enum.Parse<Key>(keyStr, true);
                uint virtualKey = (uint)KeyInterop.VirtualKeyFromKey(key);

                return RegisterHotKey(_windowHandle, HotkeyId, modifierFlags, virtualKey);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Failed to register hotkey '{hotkey}': {ex.Message}");
                return false;
            }
        }

        public void Unregister()
        {
            UnregisterHotKey(_windowHandle, HotkeyId);
        }

        public void OnHotkeyPressed()
        {
            HotkeyPressed?.Invoke();
        }

        public void Dispose()
        {
            Unregister();
        }
    }
}