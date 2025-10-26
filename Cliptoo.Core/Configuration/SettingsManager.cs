using System;
using System.IO;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;
using Cliptoo.Core.Logging;

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
                Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
                Converters = { new JsonStringEnumConverter() }
            };
        }

        public Settings Load()
        {
            LogManager.LogDebug($"Loading settings from: {_settingsPath}");
            if (!File.Exists(_settingsPath))
            {
                LogManager.LogInfo("Settings file not found, creating new default settings.");
                return new Settings(); // Return defaults
            }

            try
            {
                var json = File.ReadAllText(_settingsPath);
                var settings = JsonSerializer.Deserialize<Settings>(json, _options) ?? new Settings();
                LogManager.LogDebug("Settings loaded successfully.");
                return settings;
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException or JsonException or NotSupportedException)
            {
                LogManager.LogCritical(ex, "Failed to load settings due to corruption or read error. Backing up and using defaults.");
                try
                {
                    var backupPath = Path.ChangeExtension(_settingsPath, $".json.bak.{DateTime.Now:yyyyMMddHHmmss}");
                    File.Move(_settingsPath, backupPath);
                    LogManager.LogInfo($"Corrupt settings file backed up to: {backupPath}");
                }
                catch (Exception backupEx) when (backupEx is IOException or UnauthorizedAccessException or System.Security.SecurityException)
                {
                    LogManager.LogCritical(backupEx, "Failed to back up corrupt settings file.");
                }
                return new Settings();
            }
        }

        public void Save(Settings settings)
        {
            LogManager.LogDebug("Attempting to save settings.");
            try
            {
                var json = JsonSerializer.Serialize(settings, _options);
                var tempPath = _settingsPath + ".tmp";

                File.WriteAllText(tempPath, json);
                File.Move(tempPath, _settingsPath, true);

                LogManager.LogDebug("Settings saved successfully.");
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException or NotSupportedException)
            {
                LogManager.LogCritical(ex, "Failed to save settings.");
            }
        }
    }
}