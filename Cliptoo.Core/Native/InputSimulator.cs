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
        private const int PollingTimeoutMs = 2500;
        private const int PollingIntervalMs = 20;
        private const int FocusSettleDelayMs = 50;
        private const int DelayForStateResetMs = 200;
        private const int DelayForCtrlRegistrationMs = 30;
        private static readonly int InputSize = Marshal.SizeOf<Win32.INPUT>();

        private static Win32.INPUT CreateKeyInput(ushort virtualKeyCode, uint flags)
        {
            return new Win32.INPUT
            {
                type = Win32.INPUT_KEYBOARD,
                u = new Win32.InputUnion { ki = new Win32.KEYBDINPUT { wVk = virtualKeyCode, dwFlags = flags } }
            };
        }

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
                IntPtr candidateHwnd = Win32.GetForegroundWindow();
                if (candidateHwnd != IntPtr.Zero && Win32.IsWindow(candidateHwnd))
                {
                    uint threadId = Win32.GetWindowThreadProcessId(candidateHwnd, out uint candidateProcessId);

                    if (threadId != 0 && candidateProcessId != 0 && candidateProcessId != currentProcessId && Win32.IsWindowVisible(candidateHwnd) && Win32.IsWindowEnabled(candidateHwnd))
                    {
                        await Task.Delay(FocusSettleDelayMs, cancellationToken).ConfigureAwait(false);
                        if (cancellationToken.IsCancellationRequested) break;

                        if (Win32.GetForegroundWindow() == candidateHwnd)
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
            var resetInputs = new List<Win32.INPUT>();
            if ((Win32.GetAsyncKeyState(Win32.VK_CONTROL) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(Win32.VK_CONTROL, Win32.KEYEVENTF_KEYUP));
            if ((Win32.GetAsyncKeyState(Win32.VK_MENU) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(Win32.VK_MENU, Win32.KEYEVENTF_KEYUP));
            if ((Win32.GetAsyncKeyState(Win32.VK_SHIFT) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(Win32.VK_SHIFT, Win32.KEYEVENTF_KEYUP));
            if ((Win32.GetAsyncKeyState(Win32.VK_LWIN) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(Win32.VK_LWIN, Win32.KEYEVENTF_KEYUP));
            if ((Win32.GetAsyncKeyState(Win32.VK_RWIN) & 0x8000) != 0) resetInputs.Add(CreateKeyInput(Win32.VK_RWIN, Win32.KEYEVENTF_KEYUP));

            if (resetInputs.Count > 0)
            {
                Win32.SendInput((uint)resetInputs.Count, resetInputs.ToArray(), InputSize);
            }

            await Task.Delay(DelayForStateResetMs, cancellationToken).ConfigureAwait(false);
            if (cancellationToken.IsCancellationRequested) return;

            var ctrlDownInput = new Win32.INPUT[] { CreateKeyInput(Win32.VK_CONTROL, 0) };
            if (Win32.SendInput((uint)ctrlDownInput.Length, ctrlDownInput, InputSize) != ctrlDownInput.Length) return;

            await Task.Delay(DelayForCtrlRegistrationMs, cancellationToken).ConfigureAwait(false);
            if (cancellationToken.IsCancellationRequested) return;

            var vSequence = new Win32.INPUT[] { CreateKeyInput(Win32.VK_V, 0), CreateKeyInput(Win32.VK_V, Win32.KEYEVENTF_KEYUP) };
            Win32.SendInput((uint)vSequence.Length, vSequence, InputSize);

            var ctrlUpInput = new Win32.INPUT[] { CreateKeyInput(Win32.VK_CONTROL, Win32.KEYEVENTF_KEYUP) };
            Win32.SendInput((uint)ctrlUpInput.Length, ctrlUpInput, InputSize);
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
                uint threadId = Win32.GetWindowThreadProcessId(hwnd, out uint pid);
                if (threadId == 0 || pid == 0) return false;

                hProcess = Win32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
                if (hProcess == IntPtr.Zero) return false;

                if (!Win32.OpenProcessToken(hProcess, TOKEN_QUERY, out hToken)) return false;

                uint tmlSize;
                Win32.GetTokenInformation(hToken, Win32.TOKEN_INFORMATION_CLASS.TokenIntegrityLevel, IntPtr.Zero, 0, out tmlSize);

                pTml = Marshal.AllocHGlobal((int)tmlSize);
                if (!Win32.GetTokenInformation(hToken, Win32.TOKEN_INFORMATION_CLASS.TokenIntegrityLevel, pTml, tmlSize, out _)) return false;

                var tml = Marshal.PtrToStructure<Win32.TOKEN_MANDATORY_LABEL>(pTml);
                IntPtr pSid = Win32.GetSidSubAuthority(tml.Label.Sid, 0);
                if (pSid != IntPtr.Zero)
                {
                    return Marshal.ReadInt32(pSid) >= SECURITY_MANDATORY_HIGH_RID;
                }

                return false;
            }
            finally
            {
                if (hProcess != IntPtr.Zero) Win32.CloseHandle(hProcess);
                if (hToken != IntPtr.Zero) Win32.CloseHandle(hToken);
                if (pTml != IntPtr.Zero) Marshal.FreeHGlobal(pTml);
            }
        }
    }
}
