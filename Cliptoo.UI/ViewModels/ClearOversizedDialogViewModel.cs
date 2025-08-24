using Cliptoo.UI.ViewModels.Base;

namespace Cliptoo.UI.ViewModels
{
    public class ClearOversizedDialogViewModel : ViewModelBase
    {
        private uint _sizeMb = 10;
        public uint SizeMb { get => _sizeMb; set => SetProperty(ref _sizeMb, value); }
    }
}