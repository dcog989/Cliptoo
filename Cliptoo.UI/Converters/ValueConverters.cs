using System.Diagnostics.CodeAnalysis;
using System.Globalization;
using System.Windows;
using System.Windows.Data;
using System.Windows.Media;
using Cliptoo.Core;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;

namespace Cliptoo.UI.Converters
{
    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class ColorToBrushConverter : IValueConverter
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

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class ColorToTransparencyVisibilityConverter : IValueConverter
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

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class IsTargetedForHotkeyCaptureConverter : IMultiValueConverter
    {
        public object Convert(object[] values, Type targetType, object parameter, CultureInfo culture)
        {
            ArgumentNullException.ThrowIfNull(values);

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


    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    [ValueConversion(typeof(bool), typeof(Visibility))]
    internal sealed class BooleanToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            bool flag = value is bool b && b;
            if (parameter is string s && s.Equals(AppConstants.ConverterParameters.Inverse, StringComparison.OrdinalIgnoreCase))
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

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class EqualityToVisibilityConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            bool isEqual = value?.ToString()?.Equals(parameter?.ToString(), StringComparison.OrdinalIgnoreCase) ?? false;
            return isEqual ? Visibility.Visible : Visibility.Collapsed;
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }
    }

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class StringIsNotNullOrEmptyConverter : IValueConverter
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

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class EqualityToBooleanConverter : IValueConverter
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

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class EqualityMultiConverter : IMultiValueConverter
    {
        public object Convert(object[] values, Type targetType, object parameter, CultureInfo culture)
        {
            ArgumentNullException.ThrowIfNull(values);

            if (values.Length < 2 || values.Any(v => v == DependencyProperty.UnsetValue))
                return false;
            return object.Equals(values[0], values[1]);
        }

        public object[] ConvertBack(object value, Type[] targetTypes, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }
    }

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class PaddingSizeToThicknessConverter : IValueConverter
    {
        private static readonly Thickness Compact = new(2);
        private static readonly Thickness Standard = new(4);
        private static readonly Thickness Luxury = new(8);

        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string paddingString)
            {
                return Standard;
            }

            return paddingString.ToUpperInvariant() switch
            {
                "COMPACT" => Compact,
                "LUXURY" => Luxury,
                _ => Standard,
            };
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class ClipTypeToFriendlyNameConverter : IValueConverter
    {
        public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
        {
            if (value is not string clipType) return "Unknown";

            return FormatUtils.GetFriendlyClipTypeName(clipType);
        }

        public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
        {
            throw new NotImplementedException();
        }
    }

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class NullToVisibilityConverter : IValueConverter
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

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class IndexToVisibilityConverter : IValueConverter
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

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class MultiplyConverter : IValueConverter
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

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class AnyTrueToVisibilityConverter : IMultiValueConverter
    {
        public object Convert(object[] values, Type targetType, object parameter, CultureInfo culture)
        {
            ArgumentNullException.ThrowIfNull(values);

            if (values.Any(v => v == DependencyProperty.UnsetValue))
            {
                return Visibility.Collapsed;
            }

            foreach (var value in values)
            {
                if (value is bool b && b)
                {
                    return Visibility.Visible;
                }
                if (value is int count && count > 0)
                {
                    return Visibility.Visible;
                }
            }
            return Visibility.Collapsed;
        }

        public object[] ConvertBack(object value, Type[] targetTypes, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }
    }

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class AllTrueToVisibilityConverter : IMultiValueConverter
    {
        public object Convert(object[] values, Type targetType, object parameter, CultureInfo culture)
        {
            if (values.Any(v => v == DependencyProperty.UnsetValue))
            {
                return Visibility.Collapsed;
            }
            return values.All(v => v is bool b && b) ? Visibility.Visible : Visibility.Collapsed;
        }

        public object[] ConvertBack(object value, Type[] targetTypes, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }

    }

    [SuppressMessage("Performance", "CA1812:Avoid uninstantiated internal classes")]
    internal sealed class MultiCommandParameterConverter : IMultiValueConverter
    {
        public object Convert(object[] values, Type targetType, object parameter, CultureInfo culture)
        {
            return values.Clone();
        }

        public object[] ConvertBack(object value, Type[] targetTypes, object parameter, CultureInfo culture)
        {
            throw new NotSupportedException();
        }
    }
}