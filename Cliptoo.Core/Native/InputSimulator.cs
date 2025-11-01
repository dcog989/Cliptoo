using System;
using System.Collections.Generic;
using System.Diagnostics;
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

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll")]
        private static extern uint GetCurrentThreadId();

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
            const int PastePollingTimeoutMs = 1000;
            const int PollingIntervalMs = 30;
            const int DelayForFocusChangeMs = 75;
            const int DelayForModifierReleaseMs = 50;

            LogManager.LogDebug("PASTE_DIAG: Waiting for a valid paste target window...");
            var stopwatch = Stopwatch.StartNew();
            uint currentProcessId = (uint)Environment.ProcessId;
            IntPtr targetHwnd = IntPtr.Zero;

            // Poll for up to 1 second to find a suitable window to paste into.
            while (stopwatch.ElapsedMilliseconds < PastePollingTimeoutMs)
            {
                IntPtr hwnd = GetForegroundWindow();
                _ = GetWindowThreadProcessId(hwnd, out uint foregroundProcessId);

                // Check if the foreground window is not Cliptoo and is ready for input.
                if (foregroundProcessId != 0 && foregroundProcessId != currentProcessId && IsWindowVisible(hwnd) && IsWindowEnabled(hwnd))
                {
                    targetHwnd = hwnd;
                    break; // Found a valid, ready target.
                }
                await Task.Delay(PollingIntervalMs).ConfigureAwait(false);
            }
            stopwatch.Stop();

            if (targetHwnd == IntPtr.Zero)
            {
                LogManager.LogWarning($"PASTE_DIAG: Paste aborted. No valid target window found after polling for {stopwatch.ElapsedMilliseconds}ms.");
                return;
            }

            // Log final state for diagnostics.
            uint targetThreadId = GetWindowThreadProcessId(targetHwnd, out uint finalPid);
            var finalProcessName = ProcessUtils.GetForegroundWindowProcessName();
            var finalBuffer = new char[256];
            _ = GetWindowText(targetHwnd, finalBuffer, finalBuffer.Length);
            var finalWindowTitle = new string(finalBuffer).TrimEnd('\0');
            LogManager.LogDebug($"PASTE_DIAG: Found target window in {stopwatch.ElapsedMilliseconds}ms.");
            LogManager.LogDebug($"PASTE_DIAG: Target HWND: {targetHwnd}, PID: {finalPid}, Process: '{finalProcessName ?? "Unknown"}', Title: '{finalWindowTitle}'");


            uint currentThreadId = GetCurrentThreadId();
            bool attached = false;
            try
            {
                // Attach to the target window's input thread. This helps ensure the input goes to the right place.
                attached = AttachThreadInput(currentThreadId, targetThreadId, true);
                if (attached)
                {
                    LogManager.LogDebug("PASTE_DIAG: Successfully attached thread input.");
                }
                else
                {
                    int errorCode = Marshal.GetLastWin32Error();
                    LogManager.LogWarning($"PASTE_DIAG: Failed to attach thread input with Win32 error code: {errorCode}. Proceeding without attachment.");
                }


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
                    await Task.Delay(DelayForModifierReleaseMs).ConfigureAwait(false); // Give a moment for the OS to process the key-up events
                }

                // A small final delay to allow the target application's message queue to process the focus change.
                await Task.Delay(DelayForFocusChangeMs).ConfigureAwait(false);

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
            finally
            {
                if (attached)
                {
                    if (!AttachThreadInput(currentThreadId, targetThreadId, false))
                    {
                        // Log if detach fails, but don't treat it as a critical error.
                        int errorCode = Marshal.GetLastWin32Error();
                        LogManager.LogWarning($"PASTE_DIAG: Failed to detach thread input with Win32 error code: {errorCode}");
                    }
                    else
                    {
                        LogManager.LogDebug("PASTE_DIAG: Successfully detached thread input.");
                    }
                }
            }
        }

    }
}