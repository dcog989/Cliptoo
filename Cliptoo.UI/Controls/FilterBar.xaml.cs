using Cliptoo.UI.ViewModels;
using System.Windows;
using System.Windows.Controls;

namespace Cliptoo.UI.Controls
{
    public partial class FilterBar : UserControl
    {
        public FilterBar()
        {
            InitializeComponent();
        }

        private void FilterRadioButton_Click(object sender, RoutedEventArgs e)
        {
            if (sender is RadioButton { DataContext: FilterOption fo } && DataContext is MainViewModel vm)
            {
                vm.SelectedFilter = fo;
                vm.IsFilterPopupOpen = false;
            }
        }
    }
}