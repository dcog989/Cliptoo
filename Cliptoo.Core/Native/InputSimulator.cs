using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;
using System.Threading.Tasks;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Native
{
    public static class InputSimulator
    {
        // Win32 input constants
        private const uint INPUT_KEYBOARD = 1;
        private const ushort KEYEVENTF_KEYUP = 0x0002;

        // Virtual key codes
        private const ushort VK_CONTROL = 0x11;  // Ctrl key
        private const ushort VK_MENU = 0x12;     // Alt key
        private const ushort VK_SHIFT = 0x10;    // Shift key
        private const ushort VK_LWIN = 0x5B;     // Left Windows key
        private const ushort VK_RWIN = 0x5C;     // Right Windows key
        private const ushort VK_V = 0x56;        // V key

        // Timing constants
        private const int PollingTimeoutMs = 2500;
        private const int PollingIntervalMs = 20;
        private const int FocusSettleDelayMs = 50;
        private const int DelayForStateResetMs = 200;
        private const int DelayForCtrlRegistrationMs = 30;
        private static readonly int InputSize = Marshal.SizeOf<INPUT>();

        #region Structures
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

        private enum TOKEN_INFORMATION_CLASS
        {
            TokenIntegrityLevel = 25
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct TOKEN_MANDATORY_LABEL
        {
            public SID_AND_ATTRIBUTES Label;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct SID_AND_ATTRIBUTES
        {
            public IntPtr Sid;
            public uint Attributes;
        }
        #endregion

        #region P/Invoke
        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        private static extern short GetAsyncKeyState(int vKey);

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
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindow(IntPtr hWnd);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindowVisible(IntPtr hWnd);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool IsWindowEnabled(IntPtr hWnd);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr OpenProcess(uint processAccess, bool bInheritHandle, uint processId);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseHandle(IntPtr hObject);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("advapi32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool OpenProcessToken(IntPtr ProcessHandle, uint DesiredAccess, out IntPtr TokenHandle);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("advapi32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetTokenInformation(IntPtr TokenHandle, TOKEN_INFORMATION_CLASS TokenInformationClass, IntPtr TokenInformation, uint TokenInformationLength, out uint ReturnLength);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("advapi32.dll", SetLastError = true)]
        private static extern IntPtr GetSidSubAuthority(IntPtr pSid, uint nSubAuthority);
        #endregion

        private static INPUT CreateKeyInput(ushort virtualKeyCode, uint flags)
        {
            return new INPUT
            {
                type = INPUT_KEYBOARD,
                u = new InputUnion { ki = new KEYBDINPUT { wVk = virtualKeyCode, dwFlags = flags } }
            };
        }

        /// <summary>
        /// Simulates a paste command (Ctrl+V) to the active foreground window.
        /// </summary>
        /// <param name="cancellationToken">A token to cancel the operation.</param>
        /// <remarks>
        /// This method polls for a stable foreground window before sending input to avoid race conditions.
        /// It will fail to paste into elevated (administrator) applications if Cliptoo itself is not run as administrator, due to UIPI security restrictions.
        /// It will also fail to paste into secure desktops (e.g., UAC prompts, login screen).
        /// </remarks>
        public static async Task SendPasteAsync(CancellationToken cancellationToken = default)
        {
            IntPtr targetHwnd = await FindStableForegroundWindowAsync(cancellationToken).ConfigureAwait(false);
            if (cancellationToken.IsCancellationRequested) return;

            if (targetHwnd != IntPtr.Zero)
            {
                if (IsProcessElevated(targetHwnd))
                {
                    LogManager.LogWarning("PASTE_DIAG: Paste aborted. Target process is elevated, and Cliptoo is not. This action is blocked by UIPI.");
                    return;
                }
                await SendPasteKeysAsync(cancellationToken).ConfigureAwait(false);
            }
        }

        private static async Task<IntPtr> FindStableForegroundWindowAsync(CancellationToken cancellationToken)
        {
            var stopwatch = Stopwatch.StartNew();
            uint currentProcessId = (uint)Environment.ProcessId;

            while (stopwatch.ElapsedMilliseconds < PollingTimeoutMs)
            {
                if (cancellationToken.IsCancellationRequested) break;
                IntPtr candidateHwnd = GetForegroundWindow();
                if (candidateHwnd != IntPtr.Zero && IsWindow(candidateHwnd))
                {
                    uint threadId = GetWindowThreadProcessId(candidateHwnd, out uint candidateProcessId);

                    if (threadId != 0 && candidateProcessId != 0 && candidateProcessId != currentProcessId && IsWindowVisible(candidateHwnd) && IsWindowEnabled(candidateHwnd))
                    {
                        await Task.Delay(FocusSettleDelayMs, cancellationToken).ConfigureAwait(false);
                        if (cancellationToken.IsCancellationRequested) break;

                        if (GetForegroundWindow() == candidateHwnd)
                        {
                            LogManager.LogDebug($"PASTE_DIAG: Found stable target window in {stopwatch.ElapsedMilliseconds}ms.");
                            return candidateHwnd;
                        }
                    }
                }

                await Task.Delay(PollingIntervalMs, cancellationToken).ConfigureAwait(false);
            }

            LogManager.LogWarning($"PASTE_DIAG: Paste aborted. No stable target window found after polling for {stopwatch.ElapsedMilliseconds}ms.");
            return IntPtr.Zero;
        }

        private static async Task SendPasteKeysAsync(CancellationToken cancellationToken)
        {
            // 1. Reset any modifier keys that are currently pressed
            var resetInputs = new List<INPUT>();
            if ((GetAsyncKeyState(VK_CONTROL) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(VK_CONTROL, KEYEVENTF_KEYUP));
            if ((GetAsyncKeyState(VK_MENU) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(VK_MENU, KEYEVENTF_KEYUP));
            if ((GetAsyncKeyState(VK_SHIFT) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(VK_SHIFT, KEYEVENTF_KEYUP));
            if ((GetAsyncKeyState(VK_LWIN) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(VK_LWIN, KEYEVENTF_KEYUP));
            if ((GetAsyncKeyState(VK_RWIN) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(VK_RWIN, KEYEVENTF_KEYUP));

            if (resetInputs.Count > 0)
            {
                uint sent = SendInput((uint)resetInputs.Count, resetInputs.ToArray(), InputSize);
                if (sent != resetInputs.Count)
                {
                    LogManager.LogWarning($"PASTE_DIAG: Modifier key reset failed. Only sent {sent}/{resetInputs.Count} inputs. Last error: {Marshal.GetLastWin32Error()}");
                }
            }

            await Task.Delay(DelayForStateResetMs, cancellationToken).ConfigureAwait(false);
            if (cancellationToken.IsCancellationRequested) return;

            // 2. Send Ctrl down
            var ctrlDownInput = new INPUT[] { CreateKeyInput(VK_CONTROL, 0) };
            var sentCtrlDown = SendInput((uint)ctrlDownInput.Length, ctrlDownInput, InputSize);
            if (sentCtrlDown != ctrlDownInput.Length)
            {
                LogManager.LogError($"PASTE_DIAG: Ctrl-down SendInput failed. Last error: {Marshal.GetLastWin32Error()}. Aborting paste.");
                return;
            }

            await Task.Delay(DelayForCtrlRegistrationMs, cancellationToken).ConfigureAwait(false);
            if (cancellationToken.IsCancellationRequested) return;

            // 3. Send V down/up
            var vSequence = new INPUT[] { CreateKeyInput(VK_V, 0), CreateKeyInput(VK_V, KEYEVENTF_KEYUP) };
            var sentV = SendInput((uint)vSequence.Length, vSequence, InputSize);
            if (sentV != vSequence.Length)
            {
                LogManager.LogWarning($"PASTE_DIAG: V-key SendInput failed. Only sent {sentV}/{vSequence.Length} inputs. Last error: {Marshal.GetLastWin32Error()}");
            }

            // 4. Send Ctrl up (cleanup)
            var ctrlUpInput = new INPUT[] { CreateKeyInput(VK_CONTROL, KEYEVENTF_KEYUP) };
            var sentCtrlUp = SendInput((uint)ctrlUpInput.Length, ctrlUpInput, InputSize);
            if (sentCtrlUp != ctrlUpInput.Length)
            {
                LogManager.LogWarning($"PASTE_DIAG: Ctrl-up SendInput failed. Last error: {Marshal.GetLastWin32Error()}");
            }

            LogManager.LogDebug("PASTE_DIAG: Paste input sequence sent.");
        }

        private static bool IsProcessElevated(IntPtr hwnd)
        {
            const uint TOKEN_QUERY = 0x0008;
            const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;

            const int SECURITY_MANDATORY_HIGH_RID = 0x00003000;

            IntPtr hProcess = IntPtr.Zero;
            IntPtr hToken = IntPtr.Zero;
            IntPtr pTml = IntPtr.Zero;

            try
            {
                uint threadId = GetWindowThreadProcessId(hwnd, out uint pid);
                if (threadId == 0 || pid == 0) return false;

                hProcess = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
                if (hProcess == IntPtr.Zero) return false;

                if (!OpenProcessToken(hProcess, TOKEN_QUERY, out hToken)) return false;

                if (!GetTokenInformation(hToken, TOKEN_INFORMATION_CLASS.TokenIntegrityLevel, IntPtr.Zero, 0, out uint tmlSize) && Marshal.GetLastWin32Error() != 122) return false;

                pTml = Marshal.AllocHGlobal((int)tmlSize);
                if (!GetTokenInformation(hToken, TOKEN_INFORMATION_CLASS.TokenIntegrityLevel, pTml, tmlSize, out _)) return false;

                var tml = Marshal.PtrToStructure<TOKEN_MANDATORY_LABEL>(pTml);
                IntPtr pSid = GetSidSubAuthority(tml.Label.Sid, 0);
                if (pSid != IntPtr.Zero)
                {
                    int integrityLevel = Marshal.ReadInt32(pSid);
                    return integrityLevel >= SECURITY_MANDATORY_HIGH_RID;
                }

                return false;
            }
            finally
            {
                if (hProcess != IntPtr.Zero) CloseHandle(hProcess);
                if (hToken != IntPtr.Zero) CloseHandle(hToken);
                if (pTml != IntPtr.Zero) Marshal.FreeHGlobal(pTml);
            }
        }
    }
}