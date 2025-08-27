using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text.RegularExpressions;

namespace Cliptoo.Core.Services
{
    public sealed class ColorData
    {
        public byte R { get; set; }
        public byte G { get; set; }
        public byte B { get; set; }
        public byte A { get; set; }
    }

    public static class ColorParser
    {
        private static readonly double[][] SrgbToXyzMatrix = {
            new[] { 0.4123907993, 0.3575843394, 0.1804807884 },
            new[] { 0.2126390059, 0.7151686788, 0.0721923154 },
            new[] { 0.0193308187, 0.1191947798, 0.9505321522 }
        };

        private static readonly double[][] XyzToSrgbMatrix = {
            new[] {  3.240969942, -1.5373831776, -0.4986107603 },
            new[] { -0.9692436363,  1.8759675015,  0.0415550574 },
            new[] {  0.0556300797, -0.2039769589,  1.0569715142 }
        };

        private static readonly double[][] OklabXyzToLmsMatrix = {
            new[] {  0.8189330101,  0.3618667424, -0.1288597137 },
            new[] {  0.0329845436,  0.9293118715,  0.0361456387 },
            new[] {  0.0482003018,  0.2643662691,  0.633851707  }
        };

        private static readonly double[][] OklabLmsToLabMatrix = {
            new[] { 0.2104542553,  0.793617785,  -0.0040720468 },
            new[] { 1.9779984951, -2.428592205,   0.4505937099 },
            new[] { 0.0259040371,  0.7827717662, -0.808675766  }
        };

        private static readonly double[][] OklabLabToLmsMatrix = {
            new[] {  0.99999999845051981432,  0.39633777736240243769,   0.21580375730249306069 },
            new[] {  1.00000000838056630002, -0.10556134579289659905,  -0.06385417279300911922 },
            new[] {  1.00000005467234261899, -0.08948417752909546082,  -1.2914855480408174125  }
        };

        private static readonly double[][] OklabLmsToXyzMatrix = {
            new[] {  1.226879878071479,   -0.5578149965684922,  0.2813910501598616 },
            new[] { -0.04057575003935402,  1.112286829376436,  -0.07171107933708207 },
            new[] { -0.07637293665230801, -0.4214933235444953,  1.586161639400282  }
        };

        private static readonly Regex RgbRegex = new(@"^rgba?\(\s*([+\-\d.%]+)\s+([+\-\d.%]+)\s+([+\-\d.%]+)\s*(?:[\/\s]\s*([+\-\d.%]+)\s*)?\)$", RegexOptions.Compiled | RegexOptions.IgnoreCase);
        private static readonly Regex RgbLegacyRegex = new(@"^rgba?\(\s*([+\-\d.%]+)\s*,\s*([+\-\d.%]+)\s*,\s*([+\-\d.%]+)\s*(?:,\s*([+\-\d.%]+)\s*)?\)$", RegexOptions.Compiled | RegexOptions.IgnoreCase);
        private static readonly Regex HslRegex = new(@"^hsla?\(\s*([+\-\d.%a-z°]+)\s+([+\-\d.%]+)\s+([+\-\d.%]+)\s*(?:[\/\s]\s*([+\-\d.%]+)\s*)?\)$", RegexOptions.Compiled | RegexOptions.IgnoreCase);
        private static readonly Regex HslLegacyRegex = new(@"^hsla?\(\s*([+\-\d.%a-z°]+)\s*,\s*([+\-\d.%]+)\s*,\s*([+\-\d.%]+)\s*(?:,\s*([+\-\d.%]+)\s*)?\)$", RegexOptions.Compiled | RegexOptions.IgnoreCase);
        private static readonly Regex HexRegex = new(@"^#?(?:([a-f\d]{1})([a-f\d]{1})([a-f\d]{1})([a-f\d]{1})?|([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})?)$", RegexOptions.Compiled | RegexOptions.IgnoreCase);
        private static readonly Regex HexLegacyNumRegex = new(@"^0x(?:([a-f\d]{2}))?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$", RegexOptions.Compiled | RegexOptions.IgnoreCase);
        private static readonly Regex OklchRegex = new(@"^oklch\(\s*([+\-\d.%]+)\s+([+\-\d.%]+)\s+([+\-\d.%a-z°]+)\s*(?:[\/\s]\s*([+\-\d.%]+)\s*)?\)$", RegexOptions.Compiled | RegexOptions.IgnoreCase);

        private static readonly Dictionary<string, string> NamedColors = new() {
            {"aliceblue", "#f0f8ff"}, {"antiquewhite", "#faebd7"}, {"aqua", "#00ffff"}, {"aquamarine", "#7fffd4"}, {"azure", "#f0ffff"},
            {"beige", "#f5f5dc"}, {"bisque", "#ffe4c4"}, {"black", "#000000"}, {"blanchedalmond", "#ffebcd"}, {"blue", "#0000ff"},
            {"blueviolet", "#8a2be2"}, {"brown", "#a52a2a"}, {"burlywood", "#deb887"}, {"cadetblue", "#5f9ea0"}, {"chartreuse", "#7fff00"},
            {"chocolate", "#d2691e"}, {"coral", "#ff7f50"}, {"cornflowerblue", "#6495ed"}, {"cornsilk", "#fff8dc"}, {"crimson", "#dc143c"},
            {"cyan", "#00ffff"}, {"darkblue", "#00008b"}, {"darkcyan", "#008b8b"}, {"darkgoldenrod", "#b8860b"}, {"darkgray", "#a9a9a9"},
            {"darkgreen", "#006400"}, {"darkgrey", "#a9a9a9"}, {"darkkhaki", "#bdb76b"}, {"darkmagenta", "#8b008b"}, {"darkolivegreen", "#556b2f"},
            {"darkorange", "#ff8c00"}, {"darkorchid", "#9932cc"}, {"darkred", "#8b0000"}, {"darksalmon", "#e9967a"}, {"darkseagreen", "#8fbc8f"},
            {"darkslateblue", "#483d8b"}, {"darkslategray", "#2f4f4f"}, {"darkslategrey", "#2f4f4f"}, {"darkturquoise", "#00ced1"}, {"darkviolet", "#9400d3"},
            {"deeppink", "#ff1493"}, {"deepskyblue", "#00bfff"}, {"dimgray", "#696969"}, {"dimgrey", "#696969"}, {"dodgerblue", "#1e90ff"},
            {"firebrick", "#b22222"}, {"floralwhite", "#fffaf0"}, {"forestgreen", "#228b22"}, {"fuchsia", "#ff00ff"}, {"gainsboro", "#dcdcdc"},
            {"ghostwhite", "#f8f8ff"}, {"gold", "#ffd700"}, {"goldenrod", "#daa520"}, {"gray", "#808000"}, {"green", "#008000"},
            {"greenyellow", "#adff2f"}, {"grey", "#808080"}, {"honeydew", "#f0fff0"}, {"hotpink", "#ff69b4"}, {"indianred", "#cd5c5c"},
            {"indigo", "#4b0082"}, {"ivory", "#fffff0"}, {"khaki", "#f0e68c"}, {"lavender", "#e6e6fa"}, {"lavenderblush", "#fff0f5"},
            {"lawngreen", "#7cfc00"}, {"lemonchiffon", "#fffacd"}, {"lightblue", "#add8e6"}, {"lightcoral", "#f08080"}, {"lightcyan", "#e0ffff"},
            {"lightgoldenrodyellow", "#fafad2"}, {"lightgray", "#d3d3d3"}, {"lightgreen", "#90ee90"}, {"lightgrey", "#d3d3d3"}, {"lightpink", "#ffb6c1"},
            {"lightsalmon", "#ffa07a"}, {"lightseagreen", "#20b2aa"}, {"lightskyblue", "#87cefa"}, {"lightslategray", "#778899"}, {"lightslategrey", "#778899"},
            {"lightsteelblue", "#b0c4de"}, {"lightyellow", "#ffffe0"}, {"lime", "#00ff00"}, {"limegreen", "#32cd32"}, {"linen", "#faf0e6"},
            {"magenta", "#ff00ff"}, {"maroon", "#800000"}, {"mediumaquamarine", "#66cdaa"}, {"mediumblue", "#0000cd"}, {"mediumorchid", "#ba55d3"},
            {"mediumpurple", "#9370db"}, {"mediumseagreen", "#3cb371"}, {"mediumslateblue", "#7b68ee"}, {"mediumspringgreen", "#00fa9a"}, {"mediumturquoise", "#48d1cc"},
            {"mediumvioletred", "#c71585"}, {"midnightblue", "#191970"}, {"mintcream", "#f5fffa"}, {"mistyrose", "#ffe4e1"}, {"moccasin", "#ffe4b5"},
            {"navajowhite", "#ffdead"}, {"navy", "#000080"}, {"oldlace", "#fdf5e6"}, {"olive", "#808000"}, {"olivedrab", "#6b8e23"},
            {"orange", "#ffa500"}, {"orangered", "#ff4500"}, {"orchid", "#da70d6"}, {"palegoldenrod", "#eee8aa"}, {"palegreen", "#98fb98"},
            {"paleturquoise", "#afeeee"}, {"palevioletred", "#db7093"}, {"papayawhip", "#ffefd5"}, {"peachpuff", "#ffdab9"}, {"peru", "#cd853f"},
            {"pink", "#ffc0cb"}, {"plum", "#dda0dd"}, {"powderblue", "#b0e0e6"}, {"purple", "#800080"}, {"rebeccapurple", "#663399"},
            {"red", "#ff0000"}, {"rosybrown", "#bc8f8f"}, {"royalblue", "#4169e1"}, {"saddlebrown", "#8b4513"}, {"salmon", "#fa8072"},
            {"sandybrown", "#f4a460"}, {"seagreen", "#2e8b57"}, {"seashell", "#fff5ee"}, {"sienna", "#a0522d"}, {"silver", "#c0c0c0"},
            {"skyblue", "#87ceeb"}, {"slateblue", "#6a5acd"}, {"slategray", "#708090"}, {"slategrey", "#708090"}, {"snow", "#fffafa"},
            {"springgreen", "#00ff7f"}, {"steelblue", "#4682b4"}, {"tan", "#d2b48c"}, {"teal", "#008080"}, {"thistle", "#d8bfd8"},
            {"tomato", "#ff6347"}, {"transparent", "#00000000"}, {"turquoise", "#40e0d0"}, {"violet", "#ee82ee"}, {"wheat", "#f5deb3"},
            {"white", "#ffffff"}, {"whitesmoke", "#f5f5f5"}, {"yellow", "#ffff00"}, {"yellowgreen", "#9acd32"}
        };

        public static bool TryParseColor(string input, out ColorData? color)
        {
            color = null;
            if (string.IsNullOrWhiteSpace(input)) return false;

            var str = input.Trim();
            var lowerStr = str.ToUpperInvariant();
            if (NamedColors.TryGetValue(lowerStr, out var hex))
            {
                str = hex;
            }

            Match match;

            match = HexRegex.Match(str);
            if (match.Success)
            {
                // A short hex code (e.g., "F00") is only valid if it starts with '#'.
                // A long hex code (e.g., "FF0000") is valid with or without '#'.
                if (match.Groups[1].Success && !str.StartsWith('#'))
                {
                    return false;
                }

                byte r, g, b;
                byte a = 255;
                if (match.Groups[5].Success) // #RRGGBB or #RRGGBBAA
                {
                    r = byte.Parse(match.Groups[5].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                    g = byte.Parse(match.Groups[6].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                    b = byte.Parse(match.Groups[7].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                    if (match.Groups[8].Success) a = byte.Parse(match.Groups[8].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                }
                else // #RGB or #RGBA
                {
                    r = byte.Parse(match.Groups[1].Value + match.Groups[1].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                    g = byte.Parse(match.Groups[2].Value + match.Groups[2].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                    b = byte.Parse(match.Groups[3].Value + match.Groups[3].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                    if (match.Groups[4].Success) a = byte.Parse(match.Groups[4].Value + match.Groups[4].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                }
                color = new ColorData { R = r, G = g, B = b, A = a };
                return true;
            }

            match = HexLegacyNumRegex.Match(str);
            if (match.Success)
            {
                byte a = 255;
                if (match.Groups[1].Success) a = byte.Parse(match.Groups[1].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                byte r = byte.Parse(match.Groups[2].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                byte g = byte.Parse(match.Groups[3].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                byte b = byte.Parse(match.Groups[4].Value, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
                color = new ColorData { R = r, G = g, B = b, A = a };
                return true;
            }

            match = RgbRegex.Match(str) ?? RgbLegacyRegex.Match(str);
            if (match.Success)
            {
                var r = ParseCssNumber(match.Groups[1].Value);
                var g = ParseCssNumber(match.Groups[2].Value);
                var b = ParseCssNumber(match.Groups[3].Value);
                var a = match.Groups[4].Success ? ParseAlpha(match.Groups[4].Value) : 1.0;
                color = new ColorData { R = (byte)r, G = (byte)g, B = (byte)b, A = (byte)(a * 255) };
                return true;
            }

            match = HslRegex.Match(str) ?? HslLegacyRegex.Match(str);
            if (match.Success)
            {
                var h = ParseHue(match.Groups[1].Value);
                var s = ParsePercentage(match.Groups[2].Value);
                var l = ParsePercentage(match.Groups[3].Value);
                var a = match.Groups[4].Success ? ParseAlpha(match.Groups[4].Value) : 1.0;
                var (r, g, b) = HslToRgb(h, s, l);
                color = new ColorData { R = r, G = g, B = b, A = (byte)(a * 255) };
                return true;
            }

            match = OklchRegex.Match(str);
            if (match.Success)
            {
                var l = ParsePercentage(match.Groups[1].Value, 1.0); // L is 0-1
                var c = double.Parse(match.Groups[2].Value, CultureInfo.InvariantCulture);
                var h = ParseHue(match.Groups[3].Value);
                var a = match.Groups[4].Success ? ParseAlpha(match.Groups[4].Value) : 1.0;
                var (r, g, b) = OklchToRgb(l, c, h);
                color = new ColorData { R = r, G = g, B = b, A = (byte)(a * 255) };
                return true;
            }

            return false;
        }

        public static void RgbToOklch(byte r, byte g, byte b, out double l, out double c, out double h)
        {
            var r_lin = SrgbToLinear(r);
            var g_lin = SrgbToLinear(g);
            var b_lin = SrgbToLinear(b);

            var (x, y, z) = MultiplyMatrix(SrgbToXyzMatrix, (r_lin, g_lin, b_lin));
            var (lms1, lms2, lms3) = MultiplyMatrix(OklabXyzToLmsMatrix, (x, y, z));
            var (lms_p1, lms_p2, lms_p3) = (Math.Cbrt(lms1), Math.Cbrt(lms2), Math.Cbrt(lms3));
            var (lab_l, lab_a, lab_b) = MultiplyMatrix(OklabLmsToLabMatrix, (lms_p1, lms_p2, lms_p3));

            l = lab_l;
            c = Math.Sqrt(lab_a * lab_a + lab_b * lab_b);
            h = Math.Atan2(lab_b, lab_a) * 180 / Math.PI;
            if (h < 0) h += 360;
        }

        public static (byte r, byte g, byte b) OklchToRgb(double l, double c, double h)
        {
            var hRad = h * Math.PI / 180.0;
            var a = c * Math.Cos(hRad);
            var b = c * Math.Sin(hRad);
            var (lms_p1, lms_p2, lms_p3) = MultiplyMatrix(OklabLabToLmsMatrix, (l, a, b));
            var (lms1, lms2, lms3) = (lms_p1 * lms_p1 * lms_p1, lms_p2 * lms_p2 * lms_p2, lms_p3 * lms_p3 * lms_p3);
            var (x, y, z) = MultiplyMatrix(OklabLmsToXyzMatrix, (lms1, lms2, lms3));
            var (r_lin, g_lin, b_lin) = MultiplyMatrix(XyzToSrgbMatrix, (x, y, z));
            return (LinearToSrgb(r_lin), LinearToSrgb(g_lin), LinearToSrgb(b_lin));
        }

        private static double Clamp(double value, double min, double max) => Math.Max(min, Math.Min(value, max));
        private static bool IsPercentage(string s) => s.EndsWith('%');

        private static double ParseCssNumber(string value)
        {
            var str = value.Trim();
            if (IsPercentage(str))
            {
                return Clamp(double.Parse(str.TrimEnd('%'), CultureInfo.InvariantCulture) / 100.0 * 255.0, 0, 255);
            }
            return Clamp(double.Parse(str, CultureInfo.InvariantCulture), 0, 255);
        }

        private static double ParseHue(string value)
        {
            var str = value.ToUpperInvariant().Trim();
            var num = double.Parse(Regex.Match(str, @"[+\-\d.]+").Value, CultureInfo.InvariantCulture);
            var normalized = 0.0;
            if (str.Contains("deg", StringComparison.Ordinal) || !Regex.IsMatch(str, "[a-z]")) normalized = num;
            else if (str.Contains("rad", StringComparison.Ordinal)) normalized = num * 180 / Math.PI;
            else if (str.Contains("grad", StringComparison.Ordinal)) normalized = num * 0.9;
            else if (str.Contains("turn", StringComparison.Ordinal)) normalized = num * 360;
            var result = normalized % 360;
            return result < 0 ? result + 360 : result;
        }

        private static double ParsePercentage(string value, double max = 100.0)
        {
            return Clamp(double.Parse(value.Trim().TrimEnd('%'), CultureInfo.InvariantCulture), 0, max);
        }

        private static double ParseAlpha(string value)
        {
            var str = value.Trim();
            if (IsPercentage(str))
            {
                return Clamp(double.Parse(str.TrimEnd('%'), CultureInfo.InvariantCulture) / 100.0, 0, 1);
            }
            return Clamp(double.Parse(str, CultureInfo.InvariantCulture), 0, 1);
        }

        private static double SrgbToLinear(byte c)
        {
            var cn = c / 255.0;
            return cn <= 0.04045 ? cn / 12.92 : Math.Pow((cn + 0.055) / 1.055, 2.4);
        }

        private static byte LinearToSrgb(double clin)
        {
            var cs = clin <= 0.0031308 ? 12.92 * clin : 1.055 * Math.Pow(clin, 1 / 2.4) - 0.055;
            return (byte)Math.Round(Clamp(cs, 0, 1) * 255);
        }

        private static (byte r, byte g, byte b) HslToRgb(double h, double s, double l)
        {
            s /= 100.0; l /= 100.0;
            if (s == 0)
            {
                var gray = (byte)Math.Round(l * 255);
                return (gray, gray, gray);
            }
            var q = l < 0.5 ? l * (1 + s) : l + s - l * s;
            var p = 2 * l - q;
            double HueToRgb(double t)
            {
                if (t < 0) t += 1;
                if (t > 1) t -= 1;
                if (t < 1.0 / 6) return p + (q - p) * 6 * t;
                if (t < 1.0 / 2) return q;
                if (t < 2.0 / 3) return p + (q - p) * (2.0 / 3 - t) * 6;
                return p;
            }
            var h_norm = h / 360.0;
            return ((byte)Math.Round(HueToRgb(h_norm + 1.0 / 3) * 255), (byte)Math.Round(HueToRgb(h_norm) * 255), (byte)Math.Round(HueToRgb(h_norm - 1.0 / 3) * 255));
        }

        private static (double, double, double) MultiplyMatrix(double[][] matrix, (double, double, double) vector)
        {
            var (x, y, z) = vector;
            return (
                matrix[0][0] * x + matrix[0][1] * y + matrix[0][2] * z,
                matrix[1][0] * x + matrix[1][1] * y + matrix[1][2] * z,
                matrix[2][0] * x + matrix[2][1] * y + matrix[2][2] * z
            );
        }
    }
}