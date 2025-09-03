using System.Runtime.InteropServices;
using System.Windows;
using Cliptoo.Core.Configuration;
using Cliptoo.UI.Views;

namespace Cliptoo.UI.Services
{
    public interface IWindowPositioner
    {
        void PositionWindow(MainWindow window, Settings settings, bool isTrayRequest);
    }

    public class WindowPositioner : IWindowPositioner
    {
        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetCursorPos(out POINT lpPoint);

        [StructLayout(LayoutKind.Sequential)]
        public struct POINT
        {
            public int X;
            public int Y;
        }

        public void PositionWindow(MainWindow window, Settings settings, bool isTrayRequest)
        {
            var positionType = isTrayRequest ? "tray" : settings.LaunchPosition;
            var dpiScale = GetDpiScale(window);

            switch (positionType.ToLowerInvariant())
            {
                case "cursor":
                    GetCursorPos(out POINT point);
                    var cursorInDips = PointToDips(in point, dpiScale);
                    const double topMargin = 8.0;
                    double firstItemHeight = settings.ClipItemPadding switch
                    {
                        "compact" => 24.0,
                        "luxury" => 40.0,
                        _ => 32.0,
                    };
                    double verticalOffset = topMargin + (firstItemHeight / 2.0);
                    window.Left = cursorInDips.X - (window.Width / 2);
                    window.Top = cursorInDips.Y - verticalOffset;
                    break;

                case "top left":
                    window.Left = SystemParameters.WorkArea.Left;
                    window.Top = SystemParameters.WorkArea.Top;
                    break;
                case "top right":
                    window.Left = SystemParameters.WorkArea.Right - window.Width;
                    window.Top = SystemParameters.WorkArea.Top;
                    break;
                case "bottom left":
                    window.Left = SystemParameters.WorkArea.Left;
                    window.Top = SystemParameters.WorkArea.Bottom - window.Height;
                    break;
                case "bottom right":
                    window.Left = SystemParameters.WorkArea.Right - window.Width;
                    window.Top = SystemParameters.WorkArea.Bottom - window.Height;
                    break;

                case "tray":
                    var workArea = SystemParameters.WorkArea;
                    window.Left = workArea.Right - window.Width - 80;
                    window.Top = workArea.Bottom - window.Height - 80;
                    break;

                case "fixed":
                    window.Left = settings.FixedX;
                    window.Top = settings.FixedY;
                    break;

                case "center":
                default:
                    double pScreenWidth = SystemParameters.PrimaryScreenWidth;
                    double pScreenHeight = SystemParameters.PrimaryScreenHeight;
                    window.Left = (pScreenWidth / 2) - (window.Width / 2);
                    window.Top = (pScreenHeight / 2) - (window.Height / 2);
                    break;
            }

            EnsureWindowIsOnScreen(window);
        }

        private void EnsureWindowIsOnScreen(Window window)
        {
            var screenWidth = SystemParameters.VirtualScreenWidth;
            var screenHeight = SystemParameters.VirtualScreenHeight;
            if (window.Left + window.Width > screenWidth) window.Left = screenWidth - window.Width;
            if (window.Top + window.Height > screenHeight) window.Top = screenHeight - window.Height;
            if (window.Left < 0) window.Left = 0;
            if (window.Top < 0) window.Top = 0;
        }

        private double GetDpiScale(Window window)
        {
            var source = PresentationSource.FromVisual(window);
            return source?.CompositionTarget != null ? source.CompositionTarget.TransformToDevice.M11 : 1.0;
        }

        private static Point PointToDips(in POINT point, double dpiScale)
        {
            return new Point(point.X / dpiScale, point.Y / dpiScale);
        }
    }
}