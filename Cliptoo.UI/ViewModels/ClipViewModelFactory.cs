using Cliptoo.Core.Configuration;
using Cliptoo.Core.Database.Models;
using Cliptoo.Core.Interfaces;
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
        private readonly IPastingService _pastingService;
        private readonly INotificationService _notificationService;
        private readonly IClipDetailsLoader _clipDetailsLoader;
        private readonly IIconProvider _iconProvider;
        private readonly IClipDataService _clipDataService;
        private readonly IClipboardService _clipboardService;
        private readonly IThumbnailService _thumbnailService;
        private readonly IWebMetadataService _webMetadataService;

        public ClipViewModelFactory(
            IPastingService pastingService,
            INotificationService notificationService,
            IClipDetailsLoader clipDetailsLoader,
            IIconProvider iconProvider,
            IClipDataService clipDataService,
            IClipboardService clipboardService,
            IThumbnailService thumbnailService,
            IWebMetadataService webMetadataService)
        {
            _pastingService = pastingService;
            _notificationService = notificationService;
            _clipDetailsLoader = clipDetailsLoader;
            _iconProvider = iconProvider;
            _clipDataService = clipDataService;
            _clipboardService = clipboardService;
            _thumbnailService = thumbnailService;
            _webMetadataService = webMetadataService;
        }

        public ClipViewModel Create(Clip clip, Settings settings, string theme, MainViewModel mainViewModel)
        {
            ArgumentNullException.ThrowIfNull(settings);

            var vm = new ClipViewModel(clip, _pastingService, _notificationService, _clipDetailsLoader, mainViewModel, _iconProvider, _clipDataService, _clipboardService, _thumbnailService, _webMetadataService);
            vm.UpdateClip(clip, theme);
            return vm;
        }
    }
}