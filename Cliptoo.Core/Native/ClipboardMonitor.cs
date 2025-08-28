using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows;
using Cliptoo.Core.Native.Models;
using Cliptoo.Core.Services;

namespace Cliptoo.Core.Native
{
    public class ClipboardMonitor : IClipboardMonitor
    {
        public event EventHandler<ClipboardChangedEventArgs>? ClipboardChanged;

        private IntPtr _windowHandle;
        private bool _isStarted;
        private ulong _lastTextHash;
        private ulong _lastImageHash;
        private ulong _lastFileDropHash;
        private readonly HashSet<ulong> _hashesToSuppress = new();
        private bool _disposedValue;
        private readonly ManualResetEventSlim _suppressionActive = new(false);

        public ClipboardMonitor()
        {
        }

        public void SuppressNextClip(IEnumerable<ulong> hashes)
        {
            ArgumentNullException.ThrowIfNull(hashes);
            _hashesToSuppress.Clear();
            foreach (var hash in hashes)
            {
                _hashesToSuppress.Add(hash);
            }
            _suppressionActive.Set();
        }

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

        public void StopMonitoring()
        {
            if (!_isStarted) return;
            RemoveClipboardFormatListener(_windowHandle);
            _isStarted = false;
        }

        public void ProcessSystemUpdate()
        {
            if (ClipboardUtils.SafeGet(() => Clipboard.ContainsData(DataFormats.Rtf)) == true)
            {
                var rtfText = ClipboardUtils.SafeGet(() => Clipboard.GetData(DataFormats.Rtf) as string);
                if (!string.IsNullOrEmpty(rtfText))
                {
                    var rtfBytes = Encoding.UTF8.GetBytes(rtfText);
                    var currentHash = HashingUtils.ComputeHash(rtfBytes);
                    if (ShouldSuppress(currentHash)) return;

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
                        var allFilesBytes = Encoding.UTF8.GetBytes(allFiles);
                        var currentHash = HashingUtils.ComputeHash(allFilesBytes);
                        if (ShouldSuppress(currentHash)) return;

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
                        var currentHash = HashingUtils.ComputeHash(bytes);

                        if (ShouldSuppress(currentHash)) return;

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
                    var textBytes = Encoding.UTF8.GetBytes(text);
                    var currentHash = HashingUtils.ComputeHash(textBytes);
                    if (ShouldSuppress(currentHash)) return;

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

        private bool ShouldSuppress(ulong hash)
        {
            if (!_suppressionActive.IsSet)
            {
                return false;
            }

            if (_hashesToSuppress.Contains(hash))
            {
                Configuration.LogManager.LogDebug("Suppressed self-generated clip from being re-added.");
                _hashesToSuppress.Clear();
                _suppressionActive.Reset();
                return true;
            }

            Configuration.LogManager.LogDebug("A different clip was detected during the suppression window. Processing it.");
            _hashesToSuppress.Clear();
            _suppressionActive.Reset();
            return false;
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _suppressionActive.Dispose();
                }

                StopMonitoring();
                _disposedValue = true;
            }
        }

        public void Dispose()
        {
            Dispose(disposing: true);
            GC.SuppressFinalize(this);
        }

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool AddClipboardFormatListener(IntPtr hwnd);

        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool RemoveClipboardFormatListener(IntPtr hwnd);
    }
}