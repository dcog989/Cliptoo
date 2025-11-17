using System.Runtime.InteropServices;

namespace Cliptoo.Core.Native
{
    /// <summary>
    /// Utility methods for checking keyboard modifier key states.
    /// </summary>
    public static class KeyboardUtils
    {
        // Virtual key codes for modifier keys
        private const int VK_CONTROL = 0x11;  // Ctrl key
        private const int VK_MENU = 0x12;     // Alt key
        private const int VK_SHIFT = 0x10;    // Shift key

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        private static extern short GetAsyncKeyState(int vKey);

        /// <summary>
        /// Checks if the Control key is currently pressed.
        /// </summary>
        public static bool IsControlPressed() => (GetAsyncKeyState(VK_CONTROL) & 0x8000) != 0;

        /// <summary>
        /// Checks if the Alt key is currently pressed.
        /// </summary>
        public static bool IsAltPressed() => (GetAsyncKeyState(VK_MENU) & 0x8000) != 0;

        /// <summary>
        /// Checks if the Shift key is currently pressed.
        /// </summary>
        public static bool IsShiftPressed() => (GetAsyncKeyState(VK_SHIFT) & 0x8000) != 0;
    }
}