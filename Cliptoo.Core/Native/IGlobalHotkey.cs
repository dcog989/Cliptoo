using System;

namespace Cliptoo.Core.Native
{
    public interface IGlobalHotkey : IDisposable
    {
        /// <summary>
        /// Fired when the registered hotkey is pressed by the user.
        /// </summary>
        event EventHandler? HotkeyPressed;

        /// <summary>
        /// Registers a system-wide hotkey.
        /// </summary>
        /// <param name="hotkey">The hotkey string (e.g., "Ctrl+Alt+Q").</param>
        /// <returns>True if registration was successful, false otherwise.</returns>
        bool Register(string hotkey);

        /// <summary>
        /// Unregisters the currently active hotkey.
        /// </summary>
        void Unregister();

        /// <summary>
        /// Raises the HotkeyPressed event. Should be called from the window's message hook.
        /// </summary>
        void OnHotkeyPressed();
    }
}