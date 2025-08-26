using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
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

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr OpenProcess(ProcessAccessFlags processAccess, bool bInheritHandle, uint processId);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CloseHandle(IntPtr hObject);

        [DllImport("psapi.dll", SetLastError = true, CharSet = CharSet.Auto)]
        private static extern uint GetModuleFileNameEx(IntPtr hProcess, IntPtr hModule, [Out] StringBuilder lpBaseName, int nSize);

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

                GetWindowThreadProcessId(hwnd, out uint pid);
                if (pid == 0) return null;

                IntPtr hProcess = OpenProcess(ProcessAccessFlags.QueryLimitedInformation, false, pid);
                if (hProcess == IntPtr.Zero) return null;

                try
                {
                    var buffer = new StringBuilder(1024);
                    if (GetModuleFileNameEx(hProcess, IntPtr.Zero, buffer, buffer.Capacity) > 0)
                    {
                        return Path.GetFileName(buffer.ToString());
                    }
                }
                finally
                {
                    CloseHandle(hProcess);
                }
            }
            catch (Exception ex)
            {
                LogManager.Log(ex, "Could not get foreground window process name. This might be due to permissions.");
            }
            return null;
        }
    }
}