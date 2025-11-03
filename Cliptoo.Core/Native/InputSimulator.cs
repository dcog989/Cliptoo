using System;
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
        private const ushort VK_LWIN = 0x5B;
        private const ushort VK_RWIN = 0x5C;
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
            const int PollingIntervalMs = 20;
            const int FocusSettleDelayMs = 50;
            const int DelayForStateResetMs = 100;
            const int DelayForCtrlRegistrationMs = 30;

            var stopwatch = Stopwatch.StartNew();
            uint currentProcessId = (uint)Environment.ProcessId;
            IntPtr targetHwnd = IntPtr.Zero;

            while (stopwatch.ElapsedMilliseconds < PastePollingTimeoutMs)
            {
                IntPtr candidateHwnd = GetForegroundWindow();
                _ = GetWindowThreadProcessId(candidateHwnd, out uint candidateProcessId);

                if (candidateProcessId != 0 && candidateProcessId != currentProcessId && IsWindowVisible(candidateHwnd) && IsWindowEnabled(candidateHwnd))
                {
                    // Found a potential target. Wait a moment to see if focus settles on it.
                    await Task.Delay(FocusSettleDelayMs).ConfigureAwait(false);
                    IntPtr finalHwnd = GetForegroundWindow();

                    if (finalHwnd == candidateHwnd)
                    {
                        targetHwnd = finalHwnd;
                        break; // Focus is stable on the target window.
                    }
                }

                await Task.Delay(PollingIntervalMs).ConfigureAwait(false);
            }
            stopwatch.Stop();

            if (targetHwnd == IntPtr.Zero)
            {
                LogManager.LogWarning($"PASTE_DIAG: Paste aborted. No stable target window found after polling for {stopwatch.ElapsedMilliseconds}ms.");
                return;
            }

            uint targetThreadId = GetWindowThreadProcessId(targetHwnd, out _);
            uint currentThreadId = GetCurrentThreadId();
            bool attached = false;
            try
            {
                attached = AttachThreadInput(currentThreadId, targetThreadId, true);

                var resetInputs = new INPUT[]
                {
                    CreateKeyInput(VK_CONTROL, KEYEVENTF_KEYUP),
                    CreateKeyInput(VK_MENU, KEYEVENTF_KEYUP),
                    CreateKeyInput(VK_SHIFT, KEYEVENTF_KEYUP),
                    CreateKeyInput(VK_LWIN, KEYEVENTF_KEYUP),
                    CreateKeyInput(VK_RWIN, KEYEVENTF_KEYUP)
                };
                _ = SendInput((uint)resetInputs.Length, resetInputs, Marshal.SizeOf<INPUT>());

                await Task.Delay(DelayForStateResetMs).ConfigureAwait(false);

                var inputs = new INPUT[]
                {
                    CreateKeyInput(VK_CONTROL, 0),
                    CreateKeyInput(VK_V, 0),
                    CreateKeyInput(VK_V, KEYEVENTF_KEYUP),
                    CreateKeyInput(VK_CONTROL, KEYEVENTF_KEYUP)
                };

                // Send Ctrl+V in timed sequence for reliability.
                _ = SendInput(1, new[] { inputs[0] }, Marshal.SizeOf<INPUT>()); // Ctrl down
                await Task.Delay(DelayForCtrlRegistrationMs).ConfigureAwait(false);
                _ = SendInput(2, new[] { inputs[1], inputs[2] }, Marshal.SizeOf<INPUT>()); // V down, V up
                _ = SendInput(1, new[] { inputs[3] }, Marshal.SizeOf<INPUT>()); // Ctrl up
            }
            finally
            {
                if (attached)
                {
                    _ = AttachThreadInput(currentThreadId, targetThreadId, false);
                }
            }
        }
    }
}