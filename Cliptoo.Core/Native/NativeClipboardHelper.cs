using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Cliptoo.Core.Native
{
    internal static class NativeClipboardHelper
    {
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct DROPFILES
        {
            public int pFiles;
            public int X;
            public int Y;
            public bool fNC;
            public bool fWide;
        }

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool OpenClipboard(IntPtr hWndNewOwner);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool CloseClipboard();

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool EmptyClipboard();

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        private static extern IntPtr SetClipboardData(uint uFormat, IntPtr hMem);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr GlobalAlloc(uint uFlags, UIntPtr dwBytes);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr GlobalLock(IntPtr hMem);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GlobalUnlock(IntPtr hMem);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr GlobalFree(IntPtr hMem);

        private const uint CF_HDROP = 15;
        private const uint GMEM_MOVEABLE = 0x0002;
        private const uint GMEM_ZEROINIT = 0x0040;

        public static bool SetFileDropList(System.Collections.Specialized.StringCollection filePaths)
        {
            if (filePaths == null || filePaths.Count == 0)
            {
                return false;
            }

            // The file list must be double-null-terminated.
            var pathString = new StringBuilder();
            foreach (var path in filePaths)
            {
                pathString.Append(path);
                pathString.Append('\0');
            }
            pathString.Append('\0');

            var dropFiles = new DROPFILES
            {
                pFiles = Marshal.SizeOf<DROPFILES>(),
                fWide = true
            };

            int dropFilesSize = Marshal.SizeOf(dropFiles);
            int pathStringSize = Encoding.Unicode.GetByteCount(pathString.ToString());
            int totalSize = dropFilesSize + pathStringSize;

            IntPtr hGlobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, (UIntPtr)totalSize);
            if (hGlobal == IntPtr.Zero)
            {
                return false;
            }

            try
            {
                IntPtr pGlobal = GlobalLock(hGlobal);
                if (pGlobal == IntPtr.Zero)
                {
                    return false;
                }

                try
                {
                    Marshal.StructureToPtr(dropFiles, pGlobal, false);
                    IntPtr pathPtr = new IntPtr(pGlobal.ToInt64() + dropFilesSize);
                    Marshal.Copy(Encoding.Unicode.GetBytes(pathString.ToString()), 0, pathPtr, pathStringSize);
                }
                finally
                {
                    GlobalUnlock(hGlobal);
                }

                if (!OpenClipboard(IntPtr.Zero))
                {
                    return false;
                }

                try
                {
                    EmptyClipboard();
                    if (SetClipboardData(CF_HDROP, hGlobal) == IntPtr.Zero)
                    {
                        // SetClipboardData failed, so we must free the memory.
                        GlobalFree(hGlobal);
                        return false;
                    }
                    // SetClipboardData succeeded, the system now owns the memory.
                    hGlobal = IntPtr.Zero;
                }
                finally
                {
                    CloseClipboard();
                }
            }
            finally
            {
                if (hGlobal != IntPtr.Zero)
                {
                    GlobalFree(hGlobal);
                }
            }

            return true;
        }
    }
}