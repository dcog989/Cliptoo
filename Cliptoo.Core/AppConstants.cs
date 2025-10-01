#pragma warning disable CA1034

namespace Cliptoo.Core
{
    public static class AppConstants
    {
        public static class ClipTypes
        {
            public const string Text = "text";
            public const string Link = "link";
            public const string Color = "color";
            public const string CodeSnippet = "code_snippet";
            public const string Image = "file_image";
            public const string Video = "file_video";
            public const string Audio = "file_audio";
            public const string Archive = "file_archive";
            public const string Document = "file_document";
            public const string Dev = "file_dev";
            public const string Danger = "file_danger";
            public const string FileText = "file_text";
            public const string Generic = "file_generic";
            public const string Folder = "folder";
            public const string Rtf = "rtf";
            public const string Database = "file_database";
            public const string Font = "file_font";
            public const string FileLink = "file_link";
            public const string System = "file_system";
        }

        public static class FilterKeys
        {
            public const string All = "all";
            public const string Pinned = "pinned";
            public const string Text = "text";
            public const string Link = "link";
            public const string Image = "file_image";
            public const string Color = "color";
        }

        public static class IconKeys
        {
            public const string Pin = "pin";
            public const string Multiline = "multiline";
            public const string WasTrimmed = "was_trimmed";
            public const string Logo = "logo";
            public const string List = "list";
            public const string Error = "error";
            public const string Trash = "trash";
        }

        public static class TransformTypes
        {
            public const string Upper = "upper";
            public const string Lower = "lower";
            public const string Trim = "trim";
            public const string Capitalize = "capitalize";
            public const string Sentence = "sentence";
            public const string Invert = "invert";
            public const string Kebab = "kebab";
            public const string Snake = "snake";
            public const string Camel = "camel";
            public const string Deslug = "deslug";
            public const string Lf1 = "lf1";
            public const string Lf2 = "lf2";
            public const string RemoveLf = "remove_lf";
            public const string Timestamp = "timestamp";
        }

        public static class UITags
        {
            public const string AlwaysOnTop = "always_on_top";
            public const string ShowHide = "show_hide";
            public const string Quit = "quit";
        }

        public static class HotkeyTargets
        {
            public const string Main = "Main";
            public const string Preview = "Preview";
            public const string QuickPaste = "QuickPaste";
        }

        public static class ThemeNames
        {
            public const string System = "System";
            public const string Light = "Light";
            public const string Dark = "Dark";
        }
    }
}

#pragma warning restore CA1034
