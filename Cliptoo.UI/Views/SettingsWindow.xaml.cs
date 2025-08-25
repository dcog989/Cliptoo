using Cliptoo.UI.ViewModels;
using System.Windows;
using System.Windows.Input;
using Wpf.Ui;
using Wpf.Ui.Controls;

namespace Cliptoo.UI.Views
{
    public partial class SettingsWindow : FluentWindow
    {
        private readonly SettingsViewModel _viewModel;

        public SettingsWindow(SettingsViewModel viewModel, IContentDialogService contentDialogService)
        {
            InitializeComponent();
            _viewModel = viewModel;
            DataContext = _viewModel;
            Owner = System.Windows.Application.Current.MainWindow;

            contentDialogService.SetDialogHost(RootContentDialogPresenter);
            Loaded += async (s, e) =>
            {
                await _viewModel.InitializeAsync();
                var settings = _viewModel.Settings;
                this.Width = settings.SettingsWindowWidth;
                this.Height = settings.SettingsWindowHeight;
                if (settings.SettingsWindowX != -1 && settings.SettingsWindowY != -1)
                {
                    this.WindowStartupLocation = WindowStartupLocation.Manual;
                    this.Left = settings.SettingsWindowX;
                    this.Top = settings.SettingsWindowY;
                }
            };
        }

        protected override void OnClosed(EventArgs e)
        {
            _viewModel.Settings.SettingsWindowWidth = Math.Round(this.Width);
            _viewModel.Settings.SettingsWindowHeight = Math.Round(this.Height);
            _viewModel.Settings.SettingsWindowX = Math.Round(this.Left);
            _viewModel.Settings.SettingsWindowY = Math.Round(this.Top);
            _viewModel.SaveSettingsCommand.Execute(null);

            base.OnClosed(e);
        }

        private void Close_Click(object sender, System.Windows.RoutedEventArgs e)
        {
            _viewModel.Cleanup();
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