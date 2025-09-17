using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Timers;
using Cliptoo.Core.Logging;

namespace Cliptoo.Core.Services
{

    public sealed class FileTypeClassifier : IFileTypeClassifier, IDisposable
    {
        private class FileTypeConfig
        {
            [JsonPropertyName("version")]
            public int Version { get; set; } = 0;

            [JsonPropertyName("categories")]
            public Dictionary<string, FileTypeCategory> Categories { get; set; } = new();
        }

        private class FileTypeCategory
        {
            [JsonPropertyName("description")]
            public string Description { get; set; } = string.Empty;

            [JsonPropertyName("extensions")]
            public List<string> Extensions { get; set; } = new();
        }

        private readonly Dictionary<string, string> _extToCategory;
        private const int ConfigVersion = 2;

        private readonly string _configPath;
        private readonly FileSystemWatcher _watcher;
        private readonly System.Timers.Timer _debounceTimer;
        private bool _disposedValue;
        public event EventHandler? FileTypesChanged;

        public FileTypeClassifier(string appDataPath)
        {
            _extToCategory = new Dictionary<string, string>();
            var configFolder = Path.Combine(appDataPath, "Cliptoo");
            _configPath = Path.Combine(configFolder, "filetypes.json");

            LoadConfiguration();

            _watcher = new FileSystemWatcher(configFolder)
            {
                Filter = "filetypes.json",
                NotifyFilter = NotifyFilters.LastWrite | NotifyFilters.FileName,
                EnableRaisingEvents = true
            };
            _watcher.Changed += OnFileChanged;
            _watcher.Created += OnFileChanged;
            _watcher.Deleted += OnFileChanged;
            _watcher.Renamed += OnFileRenamed;

            _debounceTimer = new System.Timers.Timer(1000) { AutoReset = false };
            _debounceTimer.Elapsed += OnDebounceTimerElapsed;
        }

        private void OnFileRenamed(object sender, RenamedEventArgs e) => TriggerReload();
        private void OnFileChanged(object sender, FileSystemEventArgs e) => TriggerReload();
        private void TriggerReload() => _debounceTimer.Start();

        private void OnDebounceTimerElapsed(object? sender, ElapsedEventArgs e)
        {
            LoadConfiguration();
            FileTypesChanged?.Invoke(this, EventArgs.Empty);
            LogManager.LogDebug("filetypes.json changed, configuration reloaded and change event fired.");
        }

        private void LoadConfiguration()
        {
            FileTypeConfig? config = null;

            if (File.Exists(_configPath))
            {
                try
                {
                    var fileContent = File.ReadAllText(_configPath);
                    var loadedConfig = JsonSerializer.Deserialize<FileTypeConfig>(fileContent);
                    if (loadedConfig != null && loadedConfig.Version >= ConfigVersion)
                    {
                        config = loadedConfig;
                    }
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException or JsonException or NotSupportedException)
                {
                    LogManager.LogCritical(ex, $"Failed to read or parse '{_configPath}'. Will use/create default.");
                }
            }

            if (config == null)
            {
                var defaultConfigContent = GetDefaultConfigContent();
                config = JsonSerializer.Deserialize<FileTypeConfig>(defaultConfigContent);
                try
                {
                    File.WriteAllText(_configPath, defaultConfigContent);
                    LogManager.LogDebug("Wrote new/default version of filetypes.json to disk.");
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException or NotSupportedException)
                {
                    LogManager.LogCritical(ex, $"Failed to write default filetypes.json to '{_configPath}'. Will use in-memory default.");
                }
            }

            var newExtToCategory = new Dictionary<string, string>();
            if (config != null)
            {
                foreach (var (categoryName, categoryData) in config.Categories)
                {
                    foreach (var ext in categoryData.Extensions)
                    {
                        newExtToCategory[ext.TrimStart('.').ToUpperInvariant()] = categoryName;
                    }
                }
            }
            lock (_extToCategory)
            {
                _extToCategory.Clear();
                foreach (var kvp in newExtToCategory)
                {
                    _extToCategory.Add(kvp.Key, kvp.Value);
                }
            }
        }

        private static string GetDefaultConfigContent()
        {
            var assembly = Assembly.GetExecutingAssembly();
            var resourceName = "Cliptoo.Core.Services.DefaultFiletypes.json";

            using (Stream? stream = assembly.GetManifestResourceStream(resourceName))
            {
                if (stream == null)
                {
                    throw new InvalidOperationException($"Could not find embedded resource: {resourceName}");
                }
                using (var reader = new StreamReader(stream))
                {
                    return reader.ReadToEnd();
                }
            }
        }

        public string Classify(string filePath)
        {
            if (string.IsNullOrWhiteSpace(filePath))
            {
                return AppConstants.ClipTypes.Text;
            }

            var trimmedPath = filePath.Trim();

            // Prioritize classification by extension, as it works even if the file doesn't exist.
            var extension = Path.GetExtension(trimmedPath);
            if (!string.IsNullOrEmpty(extension))
            {
                string extKey = extension.TrimStart('.').ToUpperInvariant();
                lock (_extToCategory)
                {
                    if (_extToCategory.TryGetValue(extKey, out var category))
                    {
                        return category;
                    }
                }
            }

            // Fallback logic for paths without a known extension, relying on existence.
            if (Directory.Exists(trimmedPath))
            {
                return AppConstants.ClipTypes.Folder;
            }

            if (File.Exists(trimmedPath))
            {
                return AppConstants.ClipTypes.Generic;
            }

            // If it doesn't exist and we couldn't classify by extension, it's likely not a file path.
            return AppConstants.ClipTypes.Text;
        }

        private void Dispose(bool disposing)
        {
            if (!_disposedValue)
            {
                if (disposing)
                {
                    _watcher.Dispose();
                    _debounceTimer.Dispose();
                }
                _disposedValue = true;
            }
        }

        public void Dispose()
        {
            Dispose(disposing: true);
            GC.SuppressFinalize(this);
        }
    }

}