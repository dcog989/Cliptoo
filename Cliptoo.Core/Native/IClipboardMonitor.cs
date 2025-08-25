using Cliptoo.Core.Native.Models;
using System;

namespace Cliptoo.Core.Native
{
    public interface IClipboardMonitor : IDisposable
    {
        /// <summary>
        /// Fired when the system clipboard content has changed.
        /// </summary>
        event EventHandler<ClipboardChangedEventArgs> ClipboardChanged;

        /// <summary>
        /// Starts listening for clipboard updates.
        /// </summary>
        /// <param name="windowHandle">A handle to a window that can receive messages (HWND).</param>
        void Start(IntPtr windowHandle);

        /// <summary>
        /// Stops listening for clipboard updates.
        /// </summary>
        void Stop();

        /// <summary>
        /// Notifies the monitor that a system-level clipboard update has occurred.
        /// This should be called from the window message hook.
        /// </summary>
        void ProcessSystemUpdate();
        void Pause();
        void Resume();
        void SuppressNextClip(ulong hash);
    }
}