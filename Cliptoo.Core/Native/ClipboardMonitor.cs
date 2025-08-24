using Cliptoo.Core.Native.Models;
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Windows;

namespace Cliptoo.Core.Native
{
    public class ClipboardMonitor : IClipboardMonitor
    {
        public event EventHandler<ClipboardChangedEventArgs>? ClipboardChanged;

        private IntPtr _windowHandle;
        private bool _isStarted;
        private ulong _lastTextHash = 0;
        private ulong _lastImageHash = 0;
        private ulong _lastFileDropHash = 0;
        private bool _isPaused;
        public void Pause() => _isPaused = true;
        public void Resume() => _isPaused = false;

        public void Start(IntPtr windowHandle)
        {
            if (_isStarted) return;
            _windowHandle = windowHandle;
            if (!AddClipboardFormatListener(_windowHandle))
            {
                throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(), "Failed to add clipboard format listener.");
            }
            _isStarted = true;
        }

        public void Stop()
        {
            if (!_isStarted) return;
            RemoveClipboardFormatListener(_windowHandle);
            _isStarted = false;
        }

        public void ProcessSystemUpdate()
        {
            if (_isPaused) return;

            if (ClipboardUtils.SafeGet(() => Clipboard.ContainsData(DataFormats.Rtf)) == true)
            {
                var rtfText = ClipboardUtils.SafeGet(() => Clipboard.GetData(DataFormats.Rtf) as string);
                if (!string.IsNullOrEmpty(rtfText))
                {
                    var currentHash = ComputeHash(Encoding.UTF8.GetBytes(rtfText));
                    if (currentHash != _lastTextHash)
                    {
                        _lastTextHash = currentHash;
                        _lastImageHash = 0;
                        _lastFileDropHash = 0;
                        var sourceApp = ProcessUtils.GetForegroundWindowProcessName();
                        ClipboardChanged?.Invoke(this, new ClipboardChangedEventArgs(rtfText, ClipboardContentType.Text, sourceApp, true));
                    }
                    return;
                }
            }

            if (ClipboardUtils.SafeGet(() => Clipboard.ContainsFileDropList()) == true)
            {
                var filePaths = ClipboardUtils.SafeGet(() => Clipboard.GetData(DataFormats.FileDrop) as string[]);
                if (filePaths != null && filePaths.Length > 0)
                {
                    var allFiles = string.Join(Environment.NewLine, filePaths);
                    if (!string.IsNullOrEmpty(allFiles))
                    {
                        var currentHash = ComputeHash(Encoding.UTF8.GetBytes(allFiles));
                        if (currentHash != _lastFileDropHash)
                        {
                            _lastFileDropHash = currentHash;
                            _lastTextHash = 0;
                            _lastImageHash = 0;
                            var sourceApp = ProcessUtils.GetForegroundWindowProcessName();
                            ClipboardChanged?.Invoke(this, new ClipboardChangedEventArgs(allFiles, ClipboardContentType.Text, sourceApp));
                        }
                    }
                    return;
                }
            }

            if (ClipboardUtils.SafeGet(() => Clipboard.ContainsImage()) == true)
            {
                var imageSource = ClipboardUtils.SafeGet(() => Clipboard.GetImage());
                if (imageSource != null)
                {
                    using (var stream = new MemoryStream())
                    {
                        var encoder = new System.Windows.Media.Imaging.PngBitmapEncoder();
                        encoder.Frames.Add(System.Windows.Media.Imaging.BitmapFrame.Create(imageSource));
                        encoder.Save(stream);
                        var bytes = stream.ToArray();
                        var currentHash = ComputeHash(bytes);

                        if (currentHash != _lastImageHash)
                        {
                            _lastImageHash = currentHash;
                            _lastTextHash = 0;
                            _lastFileDropHash = 0;
                            var sourceApp = ProcessUtils.GetForegroundWindowProcessName();
                            ClipboardChanged?.Invoke(this, new ClipboardChangedEventArgs(bytes, ClipboardContentType.Image, sourceApp));
                        }
                    }
                    return;
                }
            }

            if (ClipboardUtils.SafeGet(() => Clipboard.ContainsText()) == true)
            {
                var text = ClipboardUtils.SafeGet(() => Clipboard.GetText());
                if (!string.IsNullOrWhiteSpace(text))
                {
                    var currentHash = ComputeHash(Encoding.UTF8.GetBytes(text));
                    if (currentHash != _lastTextHash)
                    {
                        _lastTextHash = currentHash;
                        _lastImageHash = 0;
                        _lastFileDropHash = 0;
                        var sourceApp = ProcessUtils.GetForegroundWindowProcessName();
                        ClipboardChanged?.Invoke(this, new ClipboardChangedEventArgs(text, ClipboardContentType.Text, sourceApp));
                    }
                }
            }
        }

        private ulong ComputeHash(byte[] data)
        {
            using (var sha256 = SHA256.Create())
            {
                var hashBytes = sha256.ComputeHash(data);
                return BitConverter.ToUInt64(hashBytes, 0);
            }
        }

        public void Dispose()
        {
            Stop();
        }

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool AddClipboardFormatListener(IntPtr hwnd);

        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool RemoveClipboardFormatListener(IntPtr hwnd);
    }
}