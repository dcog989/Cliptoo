using Cliptoo.Core;
using Cliptoo.Core.Services;
using System.Globalization;
using System.Windows;
using System.Windows.Data;
using System.Windows.Media;

namespace Cliptoo.UI.Converters
{
    public class ColorToBrushConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string colorString || colorString.Length > 50) return Brushes.Transparent;

            if (ColorParser.TryParseColor(colorString.Trim(), out var colorData) && colorData != null)
            {
                var color = Color.FromArgb(colorData.A, colorData.R, colorData.G, colorData.B);
                var brush = new SolidColorBrush(color);
                brush.Freeze();
                return brush;
            }

            return Brushes.Transparent;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    public class ColorToTransparencyVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string colorString || colorString.Length > 50) return Visibility.Collapsed;

            if (ColorParser.TryParseColor(colorString, out var colorData) && colorData != null)
            {
                bool isTransparent = colorData.A < 255;
                return isTransparent ? Visibility.Visible : Visibility.Collapsed;
            }

            return Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    public class IsTargetedForHotkeyCaptureConverter : IMultiValueConverter
    {
        public object Convert(object[] values, Type targetType, object parameter, CultureInfo culture)
        {
            if (values.Length < 3 || values.Any(v => v == DependencyProperty.UnsetValue || v == null))
            {
                return false;
            }

            if (values[0] is bool isCapturing && values[1] is string vmTarget && values[2] is string selfTarget)
            {
                return isCapturing && vmTarget.Equals(selfTarget, StringComparison.Ordinal);
            }

            return false;
        }

        public object[] ConvertBack(object value, Type[] targetTypes, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }
    }


    [ValueConversion(typeof(bool), typeof(Visibility))]
    public class BooleanToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            bool flag = value is bool b && b;
            if (parameter is string s && s.Equals("inverse", StringComparison.OrdinalIgnoreCase))
            {
                flag = !flag;
            }
            return flag ? Visibility.Visible : Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }
    }

    public class EqualityToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            bool isEqual = value?.ToString()?.Equals(parameter?.ToString(), StringComparison.InvariantCultureIgnoreCase) ?? false;
            return isEqual ? Visibility.Visible : Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }
    }

    public class StringIsNotNullOrEmptyConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            return value is string str && !string.IsNullOrEmpty(str);
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    public class EqualityToBooleanConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            return object.Equals(value, parameter);
        }

        public object? ConvertBack(object value, Type targetType, object? parameter, CultureInfo culture)
        {
            return value is true ? parameter : Binding.DoNothing;
        }
    }

    public class EqualityMultiConverter : IMultiValueConverter
    {
        public object Convert(object[] values, Type targetType, object parameter, CultureInfo culture)
        {
            if (values.Length < 2 || values.Any(v => v == DependencyProperty.UnsetValue))
                return false;
            return object.Equals(values[0], values[1]);
        }

        public object[] ConvertBack(object value, Type[] targetTypes, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }
    }

    public class PaddingSizeToThicknessConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            return value switch
            {
                "compact" => new Thickness(0, 2, 2, 2),
                "luxury" => new Thickness(4, 8, 4, 8),
                _ => new Thickness(2, 6, 4, 6), // standard
            };
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    public class ClipTypeToFriendlyNameConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string clipType) return "Unknown";

            return clipType switch
            {
                AppConstants.ClipTypes.Text => "Plain Text",
                AppConstants.ClipTypes.Rtf => "Formatted Text",
                AppConstants.ClipTypes.Link => "Web Link",
                AppConstants.ClipTypes.Color => "Color Code",
                AppConstants.ClipTypes.Image => "Image File",
                AppConstants.ClipTypes.Video => "Video File",
                AppConstants.ClipTypes.Audio => "Audio File",
                AppConstants.ClipTypes.Archive => "Archive File",
                AppConstants.ClipTypes.Document => "Document File",
                AppConstants.ClipTypes.Dev => "Dev File",
                AppConstants.ClipTypes.CodeSnippet => "Code Snippet",
                AppConstants.ClipTypes.Folder => "Folder",
                AppConstants.ClipTypes.Danger => "Potentially Unsafe File",
                AppConstants.ClipTypes.FileText => "Text File",
                AppConstants.ClipTypes.Generic => "Generic File",
                AppConstants.ClipTypes.Database => "Database File",
                AppConstants.ClipTypes.Font => "Font File",
                AppConstants.ClipTypes.FileLink => "Link File",
                AppConstants.ClipTypes.System => "System File",
                _ => "Clip"
            };
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    public class NullToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            return value == null ? Visibility.Collapsed : Visibility.Visible;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    public class IndexToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is int index && index >= 1 && index <= 9)
            {
                return Visibility.Visible;
            }
            return Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    public class MultiplyConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is double val && parameter is string paramStr && double.TryParse(paramStr, NumberStyles.Any, CultureInfo.InvariantCulture, out double param))
            {
                return val * param;
            }
            return value;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

}