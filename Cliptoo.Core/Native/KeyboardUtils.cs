using System.Runtime.InteropServices;

namespace Cliptoo.Core.Native
{
    public static class KeyboardUtils
    {
        private const int VK_CONTROL = 0x11;
        private const int VK_MENU = 0x12;
        private const int VK_SHIFT = 0x10;

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        private static extern short GetAsyncKeyState(int vKey);

        public static bool IsControlPressed() => (GetAsyncKeyState(VK_CONTROL) & 0x8000) != 0;
        public static bool IsAltPressed() => (GetAsyncKeyState(VK_MENU) & 0x8000) != 0;
        public static bool IsShiftPressed() => (GetAsyncKeyState(VK_SHIFT) & 0x8000) != 0;
    }
}