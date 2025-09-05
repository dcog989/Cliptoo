using System.Windows;
using System.Windows.Input;
using Cliptoo.UI.ViewModels;
using Wpf.Ui.Appearance;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Views
{
    public partial class ClipViewerWindow : FluentWindow
    {
        public ClipViewerWindow()
        {
            InitializeComponent();
            ApplicationThemeManager.Apply(this);
            DataContextChanged += OnDataContextChanged;
            PreviewKeyDown += OnPreviewKeyDown;
            Loaded += OnLoaded;
        }
        private void OnLoaded(object sender, RoutedEventArgs e)
        {
            if (DataContext is ClipViewerViewModel vm)
            {
                var settings = vm.SettingsService.Settings;
                this.Width = settings.EditorWindowWidth;
                this.Height = settings.EditorWindowHeight;

                if (settings.EditorWindowX != -1 && settings.EditorWindowY != -1)
                {
                    this.WindowStartupLocation = WindowStartupLocation.Manual;
                    this.Left = settings.EditorWindowX;
                    this.Top = settings.EditorWindowY;
                }
            }
        }

        private void OnPreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Escape)
            {
                Close();
            }
        }

        private void OnDataContextChanged(object sender, DependencyPropertyChangedEventArgs e)
        {
            if (e.OldValue is ClipViewerViewModel oldVm)
            {
                oldVm.OnRequestClose -= OnRequestClose;
            }
            if (e.NewValue is ClipViewerViewModel newVm)
            {
                newVm.OnRequestClose += OnRequestClose;
            }
        }

        private void OnRequestClose()
        {
            Close();
        }

        protected override void OnClosed(EventArgs e)
        {
            DataContextChanged -= OnDataContextChanged;
            PreviewKeyDown -= OnPreviewKeyDown;
            Loaded -= OnLoaded;

            if (DataContext is ClipViewerViewModel vm)
            {
                vm.OnRequestClose -= OnRequestClose;

                if (this.WindowState == WindowState.Normal)
                {
                    var settings = vm.SettingsService.Settings;
                    settings.EditorWindowWidth = Math.Round(this.Width);
                    settings.EditorWindowHeight = Math.Round(this.Height);
                    settings.EditorWindowX = Math.Round(this.Left);
                    settings.EditorWindowY = Math.Round(this.Top);
                    vm.SettingsService.SaveSettings();
                }
            }
            base.OnClosed(e);
        }
    }
}