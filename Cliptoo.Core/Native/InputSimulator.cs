using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Native
{
    public static class InputSimulator
    {
        private const uint INPUT_KEYBOARD = 1;
        private const ushort KEYEVENTF_KEYUP = 0x0002;
        private const ushort VK_CONTROL = 0x11;
        private const ushort VK_MENU = 0x12;
        private const ushort VK_SHIFT = 0x10;
        private const ushort VK_V = 0x56;

        [StructLayout(LayoutKind.Sequential)]
        private struct INPUT
        {
            public uint type;
            public InputUnion u;
        }

        [StructLayout(LayoutKind.Explicit)]
        private struct InputUnion
        {
            [FieldOffset(0)]
            public MOUSEINPUT mi;
            [FieldOffset(0)]
            public KEYBDINPUT ki;
            [FieldOffset(0)]
            public HARDWAREINPUT hi;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct MOUSEINPUT
        {
            public int dx;
            public int dy;
            public uint mouseData;
            public uint dwFlags;
            public uint time;
            public IntPtr dwExtraInfo;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct KEYBDINPUT
        {
            public ushort wVk;
            public ushort wScan;
            public uint dwFlags;
            public uint time;
            public IntPtr dwExtraInfo;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct HARDWAREINPUT
        {
            public uint uMsg;
            public ushort wParamL;
            public ushort wParamH;
        }

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint SendInput(uint nInputs, [In] INPUT[] pInputs, int cbSize);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern int GetWindowText(IntPtr hWnd, [Out] char[] lpString, int nMaxCount);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindowVisible(IntPtr hWnd);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindowEnabled(IntPtr hWnd);

        private static INPUT CreateKeyInput(ushort virtualKeyCode, uint flags)
        {
            return new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new InputUnion { ki = new KEYBDINPUT { wVk = virtualKeyCode, dwFlags = flags } }
            };
        }
        public static async Task SendPasteAsync()
        {
            LogManager.LogDebug("PASTE_DIAG: Polling for focus change...");
            var stopwatch = System.Diagnostics.Stopwatch.StartNew();
            while (stopwatch.ElapsedMilliseconds < 500)
            {
                var foregroundProcessName = ProcessUtils.GetForegroundWindowProcessName();
                if (foregroundProcessName != "Cliptoo.exe")
                {
                    break;
                }

                IntPtr hwnd = GetForegroundWindow();
                _ = GetWindowThreadProcessId(hwnd, out uint pid);
                var buffer = new char[256];
                _ = GetWindowText(hwnd, buffer, buffer.Length);
                var windowTitle = new string(buffer).TrimEnd('\0');
                LogManager.LogDebug($"PASTE_DIAG: Waiting... Still on Cliptoo. HWND: {hwnd}, PID: {pid}, Title: '{windowTitle}'");

                await Task.Delay(20).ConfigureAwait(false);
            }
            stopwatch.Stop();

            IntPtr finalHwnd = GetForegroundWindow();
            _ = GetWindowThreadProcessId(finalHwnd, out uint finalPid);
            var finalBuffer = new char[256];
            _ = GetWindowText(finalHwnd, finalBuffer, finalBuffer.Length);
            var finalWindowTitle = new string(finalBuffer).TrimEnd('\0');
            var finalProcessName = ProcessUtils.GetForegroundWindowProcessName();
            bool isVisible = IsWindowVisible(finalHwnd);
            bool isEnabled = IsWindowEnabled(finalHwnd);

            LogManager.LogDebug($"PASTE_DIAG: Focus change detected after {stopwatch.ElapsedMilliseconds}ms.");
            LogManager.LogDebug($"PASTE_DIAG: Target HWND: {finalHwnd}, PID: {finalPid}, Process: '{finalProcessName ?? "Unknown"}', Title: '{finalWindowTitle}', Visible: {isVisible}, Enabled: {isEnabled}");

            if (!isEnabled || !isVisible)
            {
                LogManager.LogDebug($"PASTE_DIAG: Paste aborted. Target window is not visible or not enabled.");
                return;
            }

            // A consistent, small delay is more reliable to ensure the target application's
            // message queue is ready for input after regaining focus, preventing a race condition.
            await Task.Delay(50).ConfigureAwait(false);
            LogManager.LogDebug("PASTE_DIAG: Added 50ms post-focus delay for responsiveness.");

            // Temporarily release any modifier keys the user is holding for Quick Paste
            var modifierReleaseInputs = new List<INPUT>();
            if (KeyboardUtils.IsControlPressed()) modifierReleaseInputs.Add(CreateKeyInput(VK_CONTROL, KEYEVENTF_KEYUP));
            if (KeyboardUtils.IsAltPressed()) modifierReleaseInputs.Add(CreateKeyInput(VK_MENU, KEYEVENTF_KEYUP));
            if (KeyboardUtils.IsShiftPressed()) modifierReleaseInputs.Add(CreateKeyInput(VK_SHIFT, KEYEVENTF_KEYUP));

            if (modifierReleaseInputs.Count > 0)
            {
                uint releaseResult = SendInput((uint)modifierReleaseInputs.Count, modifierReleaseInputs.ToArray(), Marshal.SizeOf<INPUT>());
                if (releaseResult == 0)
                {
                    int errorCode = Marshal.GetLastWin32Error();
                    LogManager.LogError($"PASTE_DIAG: ERROR - InputSimulator: Modifier key release SendInput failed with Win32 error code: {errorCode}");
                }
                await Task.Delay(30).ConfigureAwait(false); // Give a moment for the OS to process the key-up events
            }

            INPUT[] pasteInputs =
            {
                CreateKeyInput(VK_CONTROL, 0),          // Ctrl down
                CreateKeyInput(VK_V, 0),                // V down
                CreateKeyInput(VK_V, KEYEVENTF_KEYUP),  // V up
                CreateKeyInput(VK_CONTROL, KEYEVENTF_KEYUP) // Ctrl up
            };

            LogManager.LogDebug("InputSimulator: Sending Ctrl+V input.");
            uint result = SendInput((uint)pasteInputs.Length, pasteInputs, Marshal.SizeOf<INPUT>());
            if (result == 0)
            {
                int errorCode = Marshal.GetLastWin32Error();
                LogManager.LogError($"PASTE_DIAG: ERROR - InputSimulator: SendInput failed with Win32 error code: {errorCode}");
            }
            else
            {
                LogManager.LogDebug("InputSimulator: SendInput call successful.");
            }
        }

    }
}