using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Cliptoo.Core.Native
{
    public static class NativeClipboardHelper
    {
        public static bool SetFileDropList(System.Collections.Specialized.StringCollection filePaths)
        {
            if (filePaths == null || filePaths.Count == 0) return false;

            var pathString = new StringBuilder();
            foreach (var path in filePaths)
            {
                pathString.Append(path).Append('\0');
            }
            pathString.Append('\0');

            var dropFiles = new Win32.DROPFILES { pFiles = Marshal.SizeOf<Win32.DROPFILES>(), fWide = true };
            int dropFilesSize = Marshal.SizeOf(dropFiles);
            int pathStringSize = Encoding.Unicode.GetByteCount(pathString.ToString());
            int totalSize = dropFilesSize + pathStringSize;

            IntPtr hGlobal = Win32.GlobalAlloc(Win32.GMEM_MOVEABLE | Win32.GMEM_ZEROINIT, (UIntPtr)totalSize);
            if (hGlobal == IntPtr.Zero) return false;

            try
            {
                IntPtr pGlobal = Win32.GlobalLock(hGlobal);
                if (pGlobal == IntPtr.Zero) return false;

                try
                {
                    Marshal.StructureToPtr(dropFiles, pGlobal, false);
                    IntPtr pathPtr = new IntPtr(pGlobal.ToInt64() + dropFilesSize);
                    Marshal.Copy(Encoding.Unicode.GetBytes(pathString.ToString()), 0, pathPtr, pathStringSize);
                }
                finally { Win32.GlobalUnlock(hGlobal); }

                if (!Win32.OpenClipboard(IntPtr.Zero)) return false;

                try
                {
                    Win32.EmptyClipboard();
                    if (Win32.SetClipboardData(Win32.CF_HDROP, hGlobal) == IntPtr.Zero)
                    {
                        Win32.GlobalFree(hGlobal);
                        return false;
                    }
                    hGlobal = IntPtr.Zero;
                }
                finally { Win32.CloseClipboard(); }
            }
            finally { if (hGlobal != IntPtr.Zero) Win32.GlobalFree(hGlobal); }

            return true;
        }
    }
}
