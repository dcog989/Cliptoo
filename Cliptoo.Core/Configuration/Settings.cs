using System.ComponentModel;

namespace Cliptoo.Core.Configuration
{

    public class Settings
    {
        [DefaultValue("System")]
        public string Theme { get; set; } = "System";

        [DefaultValue("")]
        public string CompareToolPath { get; set; } = "";

        [DefaultValue("Ctrl+Alt+X")]
        public string Hotkey { get; set; } = "Ctrl+Alt+X";

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

        [DefaultValue(500.0)]
        public double WindowWidth { get; set; } = 500.0;

        [DefaultValue(350.0)]
        public double WindowHeight { get; set; } = 350.0;

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
        public uint HoverPreviewDelay { get; set; } = 1350;

        [DefaultValue(400)]
        public uint HoverImagePreviewSize { get; set; } = 400;

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
    }

}