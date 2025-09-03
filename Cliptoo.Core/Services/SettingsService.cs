using System;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Interfaces;

namespace Cliptoo.Core.Services
{
    public class SettingsService : ISettingsService
    {
        private readonly ISettingsManager _settingsManager;
        private readonly Settings _settings;

        public event EventHandler? SettingsChanged;

        public Settings Settings => _settings;

        public SettingsService(ISettingsManager settingsManager)
        {
            _settingsManager = settingsManager;
            _settings = _settingsManager.Load();
        }

        public void SaveSettings()
        {
            _settingsManager.Save(_settings);
            LogManager.LoggingLevel = _settings.LoggingLevel;
            SettingsChanged?.Invoke(this, EventArgs.Empty);
        }
    }
}