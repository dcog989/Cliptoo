using System;
using System.Runtime.InteropServices;
using System.Threading;
using Cliptoo.Core.Configuration;

namespace Cliptoo.Core.Native
{
    public static class InputSimulator
    {
        private const uint INPUT_KEYBOARD = 1;
        private const ushort KEYEVENTF_KEYUP = 0x0002;
        private const ushort VK_CONTROL = 0x11;
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

        public static void SendPaste()
        {
            LogManager.LogDebug("InputSimulator: Waiting 100ms for focus change...");
            Thread.Sleep(100);

            var foregroundProcess = ProcessUtils.GetForegroundWindowProcessName();
            LogManager.LogDebug($"InputSimulator: Attempting to send paste command to foreground process: '{foregroundProcess ?? "Unknown"}'.");

            INPUT[] inputs = new INPUT[]
            {
                new INPUT
                {
                    type = INPUT_KEYBOARD,
                    u = new InputUnion { ki = new KEYBDINPUT { wVk = VK_CONTROL, dwFlags = 0 } }
                },
                new INPUT
                {
                    type = INPUT_KEYBOARD,
                    u = new InputUnion { ki = new KEYBDINPUT { wVk = VK_V, dwFlags = 0 } }
                },
                new INPUT
                {
                    type = INPUT_KEYBOARD,
                    u = new InputUnion { ki = new KEYBDINPUT { wVk = VK_V, dwFlags = KEYEVENTF_KEYUP } }
                },
                new INPUT
                {
                    type = INPUT_KEYBOARD,
                    u = new InputUnion { ki = new KEYBDINPUT { wVk = VK_CONTROL, dwFlags = KEYEVENTF_KEYUP } }
                }
            };

            LogManager.LogDebug("InputSimulator: Sending Ctrl+V input.");
            uint result = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>());
            if (result == 0)
            {
                int errorCode = Marshal.GetLastWin32Error();
                LogManager.Log($"PASTE_DIAG: ERROR - InputSimulator: SendInput failed with Win32 error code: {errorCode}");
            }
            else
            {
                LogManager.LogDebug("InputSimulator: SendInput call successful.");
            }
        }
    }
}