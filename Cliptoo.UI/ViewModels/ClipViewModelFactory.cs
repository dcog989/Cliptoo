using Cliptoo.Core;
using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Services;
using Cliptoo.UI.Services;

namespace Cliptoo.UI.ViewModels
{
    public interface IClipViewModelFactory
    {
        ClipViewModel Create(Clip clip, Settings settings, string theme, MainViewModel mainViewModel);
    }

    public class ClipViewModelFactory : IClipViewModelFactory
    {
        private readonly CliptooController _controller;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;
        private readonly IPastingService _pastingService;
        private readonly INotificationService _notificationService;
        private readonly IClipDetailsLoader _clipDetailsLoader;
        private readonly IIconProvider _iconProvider;

        public ClipViewModelFactory(
            CliptooController controller,
            IThumbnailService thumbnailService,
            IWebMetadataService webMetadataService,
            IPastingService pastingService,
            INotificationService notificationService,
            IClipDetailsLoader clipDetailsLoader,
            IIconProvider iconProvider)
        {
            _controller = controller;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
            _pastingService = pastingService;
            _notificationService = notificationService;
            _clipDetailsLoader = clipDetailsLoader;
            _iconProvider = iconProvider;
        }

        public ClipViewModel Create(Clip clip, Settings settings, string theme, MainViewModel mainViewModel)
        {
            ArgumentNullException.ThrowIfNull(settings);

            var vm = new ClipViewModel(clip, _controller, _pastingService, _notificationService, _clipDetailsLoader, settings.ClipItemPadding, mainViewModel, _iconProvider, _thumbnailService, _webMetadataService);
            vm.UpdateClip(clip, theme);
            return vm;
        }
    }
}