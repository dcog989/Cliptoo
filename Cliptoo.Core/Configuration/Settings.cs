using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace Cliptoo.Core.Configuration
{
    public class SendToTarget : INotifyPropertyChanged
    {
        private string _name = string.Empty;
        public string Name
        {
            get => _name;
            set
            {
                if (_name == value) return;
                _name = value;
                OnPropertyChanged();
            }
        }

        private string _path = string.Empty;
        public string Path
        {
            get => _path;
            set
            {
                if (_path == value) return;
                _path = value;
                OnPropertyChanged();
            }
        }

        private string _arguments = string.Empty;
        public string Arguments
        {
            get => _arguments;
            set
            {
                if (_arguments == value) return;
                _arguments = value;
                OnPropertyChanged();
            }
        }

        public event PropertyChangedEventHandler? PropertyChanged;

        protected virtual void OnPropertyChanged([CallerMemberName] string? propertyName = null)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
        }
    }

    public class Settings
    {
        [DefaultValue("System")]
        public string Theme { get; set; } = "System";

        [DefaultValue("")]
        public string CompareToolPath { get; set; } = "";

        [DefaultValue("Ctrl+Alt+Q")]
        public string Hotkey { get; set; } = "Ctrl+Alt+Q";

        [DefaultValue("#007ACC")]
        public string AccentColor { get; set; } = "#007ACC";

        [DefaultValue("cursor")]
        public string LaunchPosition { get; set; } = "cursor";

        [DefaultValue(21)]
        public uint CleanupAgeDays { get; set; } = 21;

        [DefaultValue(999)]
        public uint MaxClipsTotal { get; set; } = 999;

        [DefaultValue("Source Code Pro")]
        public string FontFamily { get; set; } = "Source Code Pro";

        [DefaultValue(14.0f)]
        public float FontSize { get; set; } = 14.0f;

        [DefaultValue(400.0)]
        public double WindowWidth { get; set; } = 400.0;

        [DefaultValue(500.0)]
        public double WindowHeight { get; set; } = 500.0;

        [DefaultValue(100)]
        public int FixedX { get; set; } = 100;

        [DefaultValue(100)]
        public int FixedY { get; set; } = 100;

        [DefaultValue("standard")]
        public string ClipItemPadding { get; set; } = "standard";

        [DefaultValue("vibrant")]
        public string AccentChromaLevel { get; set; } = "vibrant";

        [DefaultValue(true)]
        public bool DisplayLogo { get; set; } = true;

        [DefaultValue("None")]
        public string LoggingLevel { get; set; } = "None";

        [DefaultValue(false)]
        public bool IsAlwaysOnTop { get; set; } = false;

        [DefaultValue(true)]
        public bool ShowHoverPreview { get; set; } = true;

        [DefaultValue(1250)]
        public uint HoverPreviewDelay { get; set; } = 1250;

        [DefaultValue(350)]
        public uint HoverImagePreviewSize { get; set; } = 350;

        [DefaultValue("F3")]
        public string PreviewHotkey { get; set; } = "F3";

        [DefaultValue("Ctrl+Alt")]
        public string QuickPasteHotkey { get; set; } = "Ctrl+Alt";

        [DefaultValue(600)]
        public uint PreviewTooltipMaxWidth { get; set; } = 600;

        [DefaultValue(100)]
        public uint MaxClipSizeMb { get; set; } = 100;

        [DefaultValue(false)]
        public bool StartWithWindows { get; set; } = false;

        [DefaultValue("Source Code Pro")]
        public string PreviewFontFamily { get; set; } = "Source Code Pro";

        [DefaultValue(14.0f)]
        public float PreviewFontSize { get; set; } = 14.0f;

        [DefaultValue(600.0)]
        public double EditorWindowWidth { get; set; } = 600.0;

        [DefaultValue(500.0)]
        public double EditorWindowHeight { get; set; } = 500.0;

        [DefaultValue(-1.0)]
        public double EditorWindowX { get; set; } = -1.0;

        [DefaultValue(-1.0)]
        public double EditorWindowY { get; set; } = -1.0;

        [DefaultValue(false)]
        public bool RememberSearchInput { get; set; } = false;

        [DefaultValue(false)]
        public bool RememberFilterSelection { get; set; } = false;

        [DefaultValue(false)]
        public bool PasteAsPlainText { get; set; } = false;
        public List<SendToTarget> SendToTargets { get; set; } = new();
    }

}