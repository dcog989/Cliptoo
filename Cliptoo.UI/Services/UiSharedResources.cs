using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Windows.Media;
using Cliptoo.Core;

namespace Cliptoo.UI.Services
{
    public class UiSharedResources : IUiSharedResources
    {
        private readonly IIconProvider _iconProvider;

        public event PropertyChangedEventHandler? PropertyChanged;

        private ImageSource? _logoIcon;
        public ImageSource? LogoIcon { get => _logoIcon; private set => SetProperty(ref _logoIcon, value); }

        private ImageSource? _menuIcon;
        public ImageSource? MenuIcon { get => _menuIcon; private set => SetProperty(ref _menuIcon, value); }

        private ImageSource? _wasTrimmedIcon;
        public ImageSource? WasTrimmedIcon { get => _wasTrimmedIcon; private set => SetProperty(ref _wasTrimmedIcon, value); }

        private ImageSource? _multiLineIcon;
        public ImageSource? MultiLineIcon { get => _multiLineIcon; private set => SetProperty(ref _multiLineIcon, value); }

        private ImageSource? _pinIcon;
        public ImageSource? PinIcon { get => _pinIcon; private set => SetProperty(ref _pinIcon, value); }

        private ImageSource? _pinIcon16;
        public ImageSource? PinIcon16 { get => _pinIcon16; private set => SetProperty(ref _pinIcon16, value); }

        private ImageSource? _errorIcon;
        public ImageSource? ErrorIcon { get => _errorIcon; private set => SetProperty(ref _errorIcon, value); }

        public UiSharedResources(IIconProvider iconProvider)
        {
            _iconProvider = iconProvider;
        }

        public async Task InitializeAsync()
        {
            LogoIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Logo, 24).ConfigureAwait(true);
            MenuIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.List, 28).ConfigureAwait(true);
            WasTrimmedIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.WasTrimmed, 20).ConfigureAwait(true);
            MultiLineIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Multiline, 20).ConfigureAwait(true);
            PinIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Pin, 20).ConfigureAwait(true);
            PinIcon16 = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Pin, 16).ConfigureAwait(true);
            ErrorIcon = await _iconProvider.GetIconAsync(AppConstants.IconKeys.Error, 32).ConfigureAwait(true);
        }

        private void OnPropertyChanged([CallerMemberName] string? propertyName = null)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
        }

        private bool SetProperty<T>(ref T storage, T value, [CallerMemberName] string? propertyName = null)
        {
            if (Equals(storage, value)) return false;
            storage = value;
            OnPropertyChanged(propertyName);
            return true;
        }
    }
}