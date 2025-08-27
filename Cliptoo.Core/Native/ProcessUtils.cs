using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using Cliptoo.Core.Configuration;

namespace Cliptoo.Core.Native
{
    public static class ProcessUtils
    {
        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr OpenProcess(ProcessAccessFlags processAccess, bool bInheritHandle, uint processId);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseHandle(IntPtr hObject);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("psapi.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern uint GetModuleFileNameEx(IntPtr hProcess, IntPtr hModule, [Out] char[] lpBaseName, uint nSize);

        [Flags]
        private enum ProcessAccessFlags : uint
        {
            QueryLimitedInformation = 0x1000
        }

        public static string? GetForegroundWindowProcessName()
        {
            try
            {
                IntPtr hwnd = GetForegroundWindow();
                if (hwnd == IntPtr.Zero) return null;

                _ = GetWindowThreadProcessId(hwnd, out uint pid);
                if (pid == 0) return null;

                IntPtr hProcess = OpenProcess(ProcessAccessFlags.QueryLimitedInformation, false, pid);
                if (hProcess == IntPtr.Zero) return null;

                try
                {
                    var buffer = new char[1024];
                    uint length = GetModuleFileNameEx(hProcess, IntPtr.Zero, buffer, (uint)buffer.Length);
                    if (length > 0)
                    {
                        return Path.GetFileName(new string(buffer, 0, (int)length));
                    }
                }
                finally
                {
                    CloseHandle(hProcess);
                }
            }
            catch (Exception ex) when (ex is Win32Exception or NotSupportedException)
            {
                LogManager.Log(ex, "Could not get foreground window process name. This might be due to permissions.");
            }
            return null;
        }
    }
}