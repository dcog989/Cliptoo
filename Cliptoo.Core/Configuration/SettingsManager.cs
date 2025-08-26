using System;
using System.IO;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace Cliptoo.Core.Configuration
{
    public class SettingsManager : ISettingsManager
    {
        private readonly string _settingsPath;
        private readonly JsonSerializerOptions _options;

        public SettingsManager(string appDataPath)
        {
            var configFolder = Path.Combine(appDataPath, "Cliptoo");
            Directory.CreateDirectory(configFolder);
            _settingsPath = Path.Combine(configFolder, "Cliptoo-Settings.json");
            _options = new JsonSerializerOptions
            {
                WriteIndented = true,
                Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping
            };
        }

        public Settings Load()
        {
            if (!File.Exists(_settingsPath))
            {
                return new Settings(); // Return defaults
            }

            try
            {
                var json = File.ReadAllText(_settingsPath);
                return JsonSerializer.Deserialize<Settings>(json) ?? new Settings();
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException or JsonException or NotSupportedException)
            {
                LogManager.Log(ex, "Failed to load settings, using defaults.");
                return new Settings();
            }
        }

        public void Save(Settings settings)
        {
            try
            {
                var json = JsonSerializer.Serialize(settings, _options);
                File.WriteAllText(_settingsPath, json);
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException or NotSupportedException)
            {
                LogManager.Log(ex, "Failed to save settings.");
            }
        }
    }
}