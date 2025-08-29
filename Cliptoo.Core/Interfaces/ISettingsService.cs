using System;
using Cliptoo.Core.Configuration;

namespace Cliptoo.Core.Interfaces
{
    public interface ISettingsService
    {
        Settings Settings { get; }
        void SaveSettings();
        event EventHandler? SettingsChanged;
    }
}