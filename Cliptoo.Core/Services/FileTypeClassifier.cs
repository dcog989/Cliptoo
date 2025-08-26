using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Timers;
using Cliptoo.Core.Configuration;

namespace Cliptoo.Core.Services
{

    public class FileTypeClassifier : IFileTypeClassifier, IDisposable
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
        private const string DefaultFiletypeConfig = @"{
    ""version"": 2,
    ""categories"": {
        ""file_archive"": {
            ""description"": ""Compressed and disk image archives."",
            ""extensions"": ["".7z"", "".ace"", "".arj"", "".bz2"", "".cab"", "".cbr"", "".cbz"", "".gz"", "".gzip"", "".img"", "".iso"", "".lha"", "".lzh"", "".lzma"", "".rar"", "".rpm"", "".tar"", "".tbz2"", "".tgz"", "".txz"", "".xz"", "".z"", "".zip"", "".zipx""]
        },
        ""file_audio"": {
            ""description"": ""Common audio formats."",
            ""extensions"": ["".aac"", "".aif"", "".aiff"", "".au"", "".cda"", "".flac"", "".m4a"", "".m4b"", "".mid"", "".midi"", "".mp3"", "".mpa"", "".oga"", "".ogg"", "".opus"", "".ra"", "".ram"", "".wav"", "".wma"", "".wpl""]
        },
        ""file_danger"": {
            ""description"": ""Executable scripts, installers, and system files that can potentially alter the system or run code. Should be treated with caution."",
            ""extensions"": ["".action"", "".apk"", "".app"", "".applescript"", "".appref"", "".bat"", "".bashrc"", "".bin"", "".cmd"", "".com"", "".command"", "".cpl"", "".csh"", "".diagcab"", "".dmg"", "".elf"", "".exe"", "".gadget"", "".hta"", "".inf"", "".inetloc"", "".ins"", "".ipa"", "".jar"", "".job"", "".jse"", "".ksh"", "".msc"", "".msp"", "".msu"", "".mpkg"", "".pif"", "".pkg"", "".prg"", "".profile"", "".pyc"", "".reg"", "".rgs"", "".run"", "".scr"", "".scpt"", "".shb"", "".so"", "".u3p"", "".vbe"", "".vbs"", "".vbscript"", "".workflow"", "".ws"", "".wsf"", "".wsh""]
        },
        ""file_database"": {
            ""description"": ""Database files."",
            ""extensions"": ["".accdb"", "".accde"", "".db"", "".db-shm"", "".db-wal"", "".dbf"", "".frm"", "".mdb"", "".mde"", "".sql"", "".sqlite"", "".sqlite3""]
        },
        ""file_dev"": {
            ""description"": ""Source code, configuration, scripts, and development assets."",
            ""extensions"": ["".asp"", "".aspx"", "".c"", "".cc"", "".cfg"", "".clj"", "".cljs"", "".cljc"", "".class"", "".conf"", "".cpp"", "".cs"", "".csproj"", "".css"", "".dart"", "".db3"", "".dylib"", "".editorconfig"", "".el"", "".env"", "".erl"", "".ex"", "".exs"", "".fs"", "".gitignore"", "".go"", "".gradle"", "".groovy"", "".h"", "".hpp"", "".hs"", "".htaccess"", "".htm"", "".html"", "".ini"", "".java"", "".jl"", "".js"", "".json"", "".jsp"", "".jsx"", "".kt"", "".less"", "".lisp"", "".lock"", "".lua"", "".m"", "".map"", "".ml"", "".o"", "".php"", "".pl"", "".pm"", "".ps1"", "".psc1"", "".psm1"", "".psd1"", "".psgi"", "".py"", "".r"", "".rb"", "".resx"", "".rs"", "".sass"", "".scala"", "".scss"", "".sh"", "".sln"", "".svelte"", "".swift"", "".tcl"", "".toml"", "".tsx"", "".user"", "".vb"", "".vue"", "".xhtml"", "".xaml"", "".xml"", "".yaml"", "".yml"", "".zig""]
        },
        ""file_document"": {
            ""description"": ""Formatted documents, spreadsheets, presentations, and structured data files."",
            ""extensions"": ["".azw"", "".azw3"", "".csv"", "".dif"", "".djv"", "".djvu"", "".doc"", "".docm"", "".docx"", "".dot"", "".dotm"", "".dotx"", "".epub"", "".fb2"", "".ics"", "".key"", "".mht"", "".mhtml"", "".mobi"", "".odf"", "".odg"", "".odp"", "".ods"", "".odt"", "".pages"", "".pdf"", "".pdp"", "".potx"", "".pps"", "".ppsm"", "".ppsx"", "".ppt"", "".pptm"", "".pptx"", "".ps"", "".rtf"", "".sldx"", "".slk"", "".tex"", "".tsv"", "".wpd"", "".wps"", "".xlm"", "".xls"", "".xlsm"", "".xlsx"", "".xlt"", "".xltm"", "".xltx"", "".xps""]
        },
        ""file_font"": {
            ""description"": ""Font file types."",
            ""extensions"": ["".eot"", "".fnt"", "".fon"", "".otf"", "".ttc"", "".ttf"", "".woff"", "".woff2""]
        },
        ""file_image"": {
            ""description"": ""Raster, vector, and camera raw image formats."",
            ""extensions"": ["".ai"", "".avif"", "".bmp"", "".cr2"", "".cr3"", "".cur"", "".dds"", "".dng"", "".eps"", "".gif"", "".hdr"", "".heic"", "".heif"", "".ico"", "".jp2"", "".jpe"", "".jpeg"", "".jpg"", "".jpf"", "".jpm"", "".jpx"", "".jxl"", "".j2k"", "".mj2"", "".nef"", "".pcd"", "".png"", "".psb"", "".psd"", "".raw"", "".svg"", "".svgz"", "".tga"", "".tif"", "".tiff"", "".webp"", "".xcf""]
        },
        ""file_link"": {
            ""description"": ""Links, URLs."",
            ""extensions"": ["".lnk"", "".url""]
        },
        ""file_system"": {
            ""description"": ""Core operating system files, drivers, and libraries."",
            ""extensions"": ["".bak"", "".cache"", "".dat"", "".dll"", "".drv"", "".icns"", "".msi"", "".mui"", "".partial"", "".sys"", "".tmp""]
        },
        ""file_text"": {
            ""description"": ""Plain text and markup-based text files."",
            ""extensions"": ["".log"", "".md"", "".markdown"", "".nfo"", "".txt""]
        },
        ""file_video"": {
            ""description"": ""Common video container formats."",
            ""extensions"": ["".3g2"", "".3gp"", "".asf"", "".avi"", "".divx"", "".f4v"", "".flv"", "".h264"", "".m1v"", "".m2t"", "".m2ts"", "".m2v"", "".m4v"", "".mkv"", "".mov"", "".mp4"", "".mp4v"", "".mpe"", "".mpeg"", "".mpg"", "".mts"", "".nsv"", "".ogm"", "".ogv"", "".qt"", "".rm"", "".rmvb"", "".swf"", "".tod"", "".ts"", "".vdo"", "".vob"", "".webm"", "".wmv"", "".yuv""]
        }
    }
}";

        private readonly string _configPath;
        private readonly FileSystemWatcher _watcher;
        private readonly System.Timers.Timer _debounceTimer;
        public event Action? FileTypesChanged;

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
            FileTypesChanged?.Invoke();
            LogManager.Log("filetypes.json changed, configuration reloaded and change event fired.");
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
                catch (Exception ex)
                {
                    LogManager.Log(ex, $"Failed to read or parse '{_configPath}'. Will use/create default.");
                }
            }

            if (config == null)
            {
                config = JsonSerializer.Deserialize<FileTypeConfig>(DefaultFiletypeConfig);
                try
                {
                    File.WriteAllText(_configPath, DefaultFiletypeConfig);
                    LogManager.Log("Wrote new/default version of filetypes.json to disk.");
                }
                catch (Exception ex)
                {
                    LogManager.Log(ex, $"Failed to write default filetypes.json to '{_configPath}'. Will use in-memory default.");
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

        public void Dispose()
        {
            _watcher.Dispose();
            _debounceTimer.Dispose();
        }
    }

}