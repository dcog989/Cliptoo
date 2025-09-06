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
            set => SetProperty(ref _name, value);
        }

        private string _path = string.Empty;
        public string Path
        {
            get => _path;
            set => SetProperty(ref _path, value);
        }

        private string _arguments = string.Empty;
        public string Arguments
        {
            get => _arguments;
            set => SetProperty(ref _arguments, value);
        }

        public event PropertyChangedEventHandler? PropertyChanged;

        protected virtual void OnPropertyChanged([CallerMemberName] string? propertyName = null)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
        }

        protected bool SetProperty<T>(ref T storage, T value, [CallerMemberName] string? propertyName = null)
        {
            if (Equals(storage, value)) return false;
            storage = value;
            OnPropertyChanged(propertyName);
            return true;
        }
    }

    public class Settings : INotifyPropertyChanged
    {
        public event PropertyChangedEventHandler? PropertyChanged;

        protected virtual void OnPropertyChanged([CallerMemberName] string? propertyName = null)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
        }

        protected bool SetProperty<T>(ref T storage, T value, [CallerMemberName] string? propertyName = null)
        {
            if (Equals(storage, value)) return false;
            storage = value;
            OnPropertyChanged(propertyName);
            return true;
        }

        #region Appearance & Theming
        private string _theme = AppConstants.ThemeNames.System;
        [DefaultValue(AppConstants.ThemeNames.System)]
        public string Theme { get => _theme; set => SetProperty(ref _theme, value); }

        private string _accentColor = "#007ACC";
        [DefaultValue("#007ACC")]
        public string AccentColor { get => _accentColor; set => SetProperty(ref _accentColor, value); }

        private string _fontFamily = "Source Code Pro";
        [DefaultValue("Source Code Pro")]
        public string FontFamily { get => _fontFamily; set => SetProperty(ref _fontFamily, value); }

        private float _fontSize = 14.0f;
        [DefaultValue(14.0f)]
        public float FontSize { get => _fontSize; set => SetProperty(ref _fontSize, value); }

        private string _clipItemPadding = "Standard";
        [DefaultValue("Standard")]
        public string ClipItemPadding { get => _clipItemPadding; set => SetProperty(ref _clipItemPadding, value); }

        private string _accentChromaLevel = "vibrant";
        [DefaultValue("vibrant")]
        public string AccentChromaLevel { get => _accentChromaLevel; set => SetProperty(ref _accentChromaLevel, value); }

        private bool _displayLogo = true;
        [DefaultValue(true)]
        public bool DisplayLogo { get => _displayLogo; set => SetProperty(ref _displayLogo, value); }
        #endregion

        #region Window Management
        private string _launchPosition = "Cursor";
        [DefaultValue("Cursor")]
        public string LaunchPosition { get => _launchPosition; set => SetProperty(ref _launchPosition, value); }

        private double _windowWidth = 400.0;
        [DefaultValue(400.0)]
        public double WindowWidth { get => _windowWidth; set => SetProperty(ref _windowWidth, value); }

        private double _windowHeight = 500.0;
        [DefaultValue(500.0)]
        public double WindowHeight { get => _windowHeight; set => SetProperty(ref _windowHeight, value); }

        private int _fixedX = 100;
        [DefaultValue(100)]
        public int FixedX { get => _fixedX; set => SetProperty(ref _fixedX, value); }

        private int _fixedY = 100;
        [DefaultValue(100)]
        public int FixedY { get => _fixedY; set => SetProperty(ref _fixedY, value); }

        private bool _isAlwaysOnTop;
        [DefaultValue(false)]
        public bool IsAlwaysOnTop { get => _isAlwaysOnTop; set => SetProperty(ref _isAlwaysOnTop, value); }

        private double _editorWindowWidth = 600.0;
        [DefaultValue(600.0)]
        public double EditorWindowWidth { get => _editorWindowWidth; set => SetProperty(ref _editorWindowWidth, value); }

        private double _editorWindowHeight = 500.0;
        [DefaultValue(500.0)]
        public double EditorWindowHeight { get => _editorWindowHeight; set => SetProperty(ref _editorWindowHeight, value); }

        private double _editorWindowX = -1.0;
        [DefaultValue(-1.0)]
        public double EditorWindowX { get => _editorWindowX; set => SetProperty(ref _editorWindowX, value); }

        private double _editorWindowY = -1.0;
        [DefaultValue(-1.0)]
        public double EditorWindowY { get => _editorWindowY; set => SetProperty(ref _editorWindowY, value); }

        private double _settingsWindowWidth = 800.0;
        [DefaultValue(800.0)]
        public double SettingsWindowWidth { get => _settingsWindowWidth; set => SetProperty(ref _settingsWindowWidth, value); }

        private double _settingsWindowHeight = 650.0;
        [DefaultValue(650.0)]
        public double SettingsWindowHeight { get => _settingsWindowHeight; set => SetProperty(ref _settingsWindowHeight, value); }

        private double _settingsWindowX = -1.0;
        [DefaultValue(-1.0)]
        public double SettingsWindowX { get => _settingsWindowX; set => SetProperty(ref _settingsWindowX, value); }

        private double _settingsWindowY = -1.0;
        [DefaultValue(-1.0)]
        public double SettingsWindowY { get => _settingsWindowY; set => SetProperty(ref _settingsWindowY, value); }
        #endregion

        #region Hotkeys
        private string _hotkey = "Ctrl+Alt+Q";
        [DefaultValue("Ctrl+Alt+Q")]
        public string Hotkey { get => _hotkey; set => SetProperty(ref _hotkey, value); }

        private string _previewHotkey = "F3";
        [DefaultValue("F3")]
        public string PreviewHotkey { get => _previewHotkey; set => SetProperty(ref _previewHotkey, value); }

        private string _quickPasteHotkey = "Ctrl+Alt";
        [DefaultValue("Ctrl+Alt")]
        public string QuickPasteHotkey { get => _quickPasteHotkey; set => SetProperty(ref _quickPasteHotkey, value); }
        #endregion

        #region Preview & Tooltip
        private bool _showHoverPreview = true;
        [DefaultValue(true)]
        public bool ShowHoverPreview { get => _showHoverPreview; set => SetProperty(ref _showHoverPreview, value); }

        private uint _hoverPreviewDelay = 1250;
        [DefaultValue(1250)]
        public uint HoverPreviewDelay { get => _hoverPreviewDelay; set => SetProperty(ref _hoverPreviewDelay, value); }

        private uint _hoverImagePreviewSize = 350;
        [DefaultValue(350)]
        public uint HoverImagePreviewSize { get => _hoverImagePreviewSize; set => SetProperty(ref _hoverImagePreviewSize, value); }

        private uint _previewTooltipMaxWidth = 600;
        [DefaultValue(600)]
        public uint PreviewTooltipMaxWidth { get => _previewTooltipMaxWidth; set => SetProperty(ref _previewTooltipMaxWidth, value); }

        private string _previewFontFamily = "Source Code Pro";
        [DefaultValue("Source Code Pro")]
        public string PreviewFontFamily { get => _previewFontFamily; set => SetProperty(ref _previewFontFamily, value); }

        private float _previewFontSize = 14.0f;
        [DefaultValue(14.0f)]
        public float PreviewFontSize { get => _previewFontSize; set => SetProperty(ref _previewFontSize, value); }
        #endregion

        #region Data Management
        private uint _cleanupAgeDays = 21;
        [DefaultValue(21)]
        public uint CleanupAgeDays { get => _cleanupAgeDays; set => SetProperty(ref _cleanupAgeDays, value); }

        private uint _maxClipsTotal = 999;
        [DefaultValue(999)]
        public uint MaxClipsTotal { get => _maxClipsTotal; set => SetProperty(ref _maxClipsTotal, value); }

        private uint _maxClipSizeMb = 100;
        [DefaultValue(100)]
        public uint MaxClipSizeMb { get => _maxClipSizeMb; set => SetProperty(ref _maxClipSizeMb, value); }
        #endregion

        #region Behavior & Functionality
        private string _compareToolPath = "";
        [DefaultValue("")]
        public string CompareToolPath { get => _compareToolPath; set => SetProperty(ref _compareToolPath, value); }

        private bool _startWithWindows;
        [DefaultValue(false)]
        public bool StartWithWindows { get => _startWithWindows; set => SetProperty(ref _startWithWindows, value); }

        private bool _rememberSearchInput;
        [DefaultValue(false)]
        public bool RememberSearchInput { get => _rememberSearchInput; set => SetProperty(ref _rememberSearchInput, value); }

        private bool _rememberFilterSelection;
        [DefaultValue(false)]
        public bool RememberFilterSelection { get => _rememberFilterSelection; set => SetProperty(ref _rememberFilterSelection, value); }

        private bool _pasteAsPlainText;
        [DefaultValue(false)]
        public bool PasteAsPlainText { get => _pasteAsPlainText; set => SetProperty(ref _pasteAsPlainText, value); }

        private bool _moveClipToTopOnPaste = true;
        [DefaultValue(true)]
        public bool MoveClipToTopOnPaste { get => _moveClipToTopOnPaste; set => SetProperty(ref _moveClipToTopOnPaste, value); }

        public List<SendToTarget> SendToTargets { get; set; } = new();
        #endregion

        #region Diagnostics
        private string _loggingLevel = "None";
        [DefaultValue("None")]
        public string LoggingLevel { get => _loggingLevel; set => SetProperty(ref _loggingLevel, value); }
        #endregion
    }
}