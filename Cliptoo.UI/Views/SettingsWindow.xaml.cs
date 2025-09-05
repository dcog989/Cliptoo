using System.Windows;
using System.Windows.Input;
using Cliptoo.UI.ViewModels;
using Wpf.Ui;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Views
{
    internal partial class SettingsWindow : FluentWindow
    {
        private readonly SettingsViewModel _viewModel;

        public SettingsWindow(SettingsViewModel viewModel, IContentDialogService contentDialogService)
        {
            InitializeComponent();
            _viewModel = viewModel;
            DataContext = _viewModel;
            Owner = System.Windows.Application.Current.MainWindow;

            contentDialogService.SetDialogHost(RootContentDialogPresenter);
            Loaded += OnLoaded;
        }

        private async void OnLoaded(object? sender, RoutedEventArgs e)
        {
            await _viewModel.InitializeAsync();
            var settings = _viewModel.Settings;
            this.Width = settings.SettingsWindowWidth;
            this.Height = settings.SettingsWindowHeight;
            if (settings.SettingsWindowX > 0 && settings.SettingsWindowY > 0)
            {
                this.WindowStartupLocation = WindowStartupLocation.Manual;
                this.Left = settings.SettingsWindowX;
                this.Top = settings.SettingsWindowY;
            }
        }

        protected override void OnClosed(EventArgs e)
        {
            base.OnClosed(e);

            var settings = _viewModel.Settings;
            if (this.WindowState == WindowState.Normal)
            {
                settings.SettingsWindowWidth = Math.Round(this.Width);
                settings.SettingsWindowHeight = Math.Round(this.Height);
                settings.SettingsWindowX = Math.Round(this.Left);
                settings.SettingsWindowY = Math.Round(this.Top);
            }

            _viewModel.SaveSettingsCommand.Execute(null);
            _viewModel.Cleanup();
            Loaded -= OnLoaded;
        }

        private void Close_Click(object sender, System.Windows.RoutedEventArgs e)
        {
            this.Close();
        }

        private void SettingsWindow_PreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Escape)
            {
                e.Handled = true;
                Close_Click(this, new RoutedEventArgs());
            }
        }
    }
}