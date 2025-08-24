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
            };
        }

        private void Close_Click(object sender, System.Windows.RoutedEventArgs e)
        {
            _viewModel.SaveSettingsCommand.Execute(null);
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