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
        private readonly System.Timers.Timer _suppressionResetTimer;
        private readonly ManualResetEventSlim _suppressionActive = new(false);


        public ClipboardMonitor()
        {
            // This timer acts as a safety net. If a clipboard update is suppressed,
            // it ensures the suppression state is eventually cleared even if subsequent
            // clipboard events don't occur.
            _suppressionResetTimer = new System.Timers.Timer(250) { AutoReset = false };
            _suppressionResetTimer.Elapsed += (s, e) =>
            {
                if (_suppressionActive.IsSet)
                {
                    Configuration.LogManager.LogDebug("Suppression window closed by timer.");
                    _hashesToSuppress.Clear();
                    _suppressionActive.Reset();
                }
            };
        }

        public void SuppressNextClip(IEnumerable<ulong> hashes)
        {
            ArgumentNullException.ThrowIfNull(hashes);
            _suppressionResetTimer.Stop();
            _hashesToSuppress.Clear();
            foreach (var hash in hashes)
            {
                _hashesToSuppress.Add(hash);
            }
            _suppressionActive.Set();
            _suppressionResetTimer.Start(); // Start the safety-net timer
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
            if (_suppressionActive.IsSet)
            {
                // During the suppression window, we check all available formats. If any match, we ignore the event
                // but keep the suppression active for the timer's duration to catch the entire flurry of events.
                if (CheckForSuppressedHashes())
                {
                    return;
                }

                // If no hash matched, it means a legitimate clip was copied immediately after we pasted ours.
                // We should process it and immediately end the suppression window.
                Configuration.LogManager.LogDebug("A different clip was detected during the suppression window. Processing it.");
                _suppressionResetTimer.Stop();
                _hashesToSuppress.Clear();
                _suppressionActive.Reset();
            }

            // If we're here, the clip was not suppressed. Now, choose the best format to process.
            var availableData = GetAvailableClipboardData();
            (string formatKey, object content, ulong hash) bestCandidate;

            if (availableData.TryGetValue(DataFormats.Rtf, out var rtfData))
            {
                bestCandidate = (DataFormats.Rtf, rtfData.Content, rtfData.Hash);
            }
            else if (availableData.TryGetValue("FileDrop", out var fileDropData))
            {
                bestCandidate = ("FileDrop", fileDropData.Content, fileDropData.Hash);
            }
            else if (availableData.TryGetValue("Image", out var imageData))
            {
                bestCandidate = ("Image", imageData.Content, imageData.Hash);
            }
            else if (availableData.TryGetValue(DataFormats.UnicodeText, out var textData))
            {
                bestCandidate = (DataFormats.UnicodeText, textData.Content, textData.Hash);
            }
            else
            {
                return; // No usable data found on clipboard.
            }

            // Check if this content is a duplicate of the *very last* item we processed.
            if (bestCandidate.formatKey == DataFormats.Rtf || bestCandidate.formatKey == DataFormats.UnicodeText)
            {
                if (bestCandidate.hash == _lastTextHash) return;
                _lastTextHash = bestCandidate.hash;
                _lastImageHash = 0;
                _lastFileDropHash = 0;
            }
            else if (bestCandidate.formatKey == "FileDrop")
            {
                if (bestCandidate.hash == _lastFileDropHash) return;
                _lastFileDropHash = bestCandidate.hash;
                _lastTextHash = 0;
                _lastImageHash = 0;
            }
            else if (bestCandidate.formatKey == "Image")
            {
                if (bestCandidate.hash == _lastImageHash) return;
                _lastImageHash = bestCandidate.hash;
                _lastTextHash = 0;
                _lastFileDropHash = 0;
            }

            // Fire the event with the chosen content.
            var sourceApp = ProcessUtils.GetForegroundWindowProcessName();
            ClipboardChangedEventArgs eventArgs;

            if (bestCandidate.formatKey == DataFormats.Rtf)
            {
                eventArgs = new ClipboardChangedEventArgs(bestCandidate.content, ClipboardContentType.Text, sourceApp, true);
            }
            else if (bestCandidate.formatKey == "FileDrop" || bestCandidate.formatKey == DataFormats.UnicodeText)
            {
                eventArgs = new ClipboardChangedEventArgs(bestCandidate.content, ClipboardContentType.Text, sourceApp, false);
            }
            else if (bestCandidate.formatKey == "Image")
            {
                eventArgs = new ClipboardChangedEventArgs(bestCandidate.content, ClipboardContentType.Image, sourceApp);
            }
            else
            {
                return;
            }

            ClipboardChanged?.Invoke(this, eventArgs);
        }

        private bool CheckForSuppressedHashes()
        {
            var availableData = GetAvailableClipboardData();
            foreach (var format in availableData)
            {
                if (_hashesToSuppress.Contains(format.Value.Hash))
                {
                    Configuration.LogManager.LogDebug($"Suppressed self-generated clip based on format '{format.Key}'.");
                    return true;
                }
            }
            return false;
        }

        private static Dictionary<string, (object Content, ulong Hash)> GetAvailableClipboardData()
        {
            var availableData = new Dictionary<string, (object Content, ulong Hash)>();

            if (ClipboardUtils.SafeGet(() => Clipboard.ContainsData(DataFormats.Rtf)) == true)
            {
                var rtfText = ClipboardUtils.SafeGet(() => Clipboard.GetData(DataFormats.Rtf) as string);
                if (!string.IsNullOrEmpty(rtfText))
                {
                    availableData[DataFormats.Rtf] = (rtfText, HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(rtfText)));
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
                        availableData["FileDrop"] = (allFiles, HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(allFiles)));
                    }
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
                        availableData["Image"] = (bytes, HashingUtils.ComputeHash(bytes));
                    }
                }
            }

            if (ClipboardUtils.SafeGet(() => Clipboard.ContainsText()) == true)
            {
                var text = ClipboardUtils.SafeGet(() => Clipboard.GetText());
                if (!string.IsNullOrWhiteSpace(text))
                {
                    availableData[DataFormats.UnicodeText] = (text, HashingUtils.ComputeHash(Encoding.UTF8.GetBytes(text)));
                }
            }
            return availableData;
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _suppressionResetTimer.Dispose();
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