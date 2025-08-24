using Cliptoo.Core;
using Cliptoo.Core.Services;
using Cliptoo.UI.Helpers;
using Cliptoo.UI.ViewModels;
using SixLabors.ImageSharp;
using System.IO;
using System.Text;

namespace Cliptoo.UI.Services
{
    public class ClipDetailsLoader : IClipDetailsLoader
    {
        public async Task<string?> GetThumbnailAsync(ClipViewModel vm, IThumbnailService thumbnailService, IWebMetadataService webMetadataService, string theme)
        {
            var extension = Path.GetExtension(vm.Content)?.ToLowerInvariant() ?? string.Empty;

            if (vm.ClipType == AppConstants.ClipTypes.Image)
            {
                return await thumbnailService.GetThumbnailAsync(vm.Content, extension == ".svg" ? theme : null);
            }
            if (vm.ClipType == AppConstants.ClipTypes.Link)
            {
                return await webMetadataService.GetFaviconAsync(vm.Content);
            }
            return null;
        }

        public async Task<string?> GetImagePreviewAsync(ClipViewModel vm, IThumbnailService thumbnailService, uint size, string theme)
        {
            if (!vm.IsImage) return null;

            var extension = Path.GetExtension(vm.Content)?.ToLowerInvariant() ?? string.Empty;
            return await thumbnailService.GetImagePreviewAsync(vm.Content, size, extension == ".svg" ? theme : null);
        }

        public async Task<string?> GetPageTitleAsync(ClipViewModel vm, IWebMetadataService webMetadataService, CancellationToken token)
        {
            if (!vm.IsLinkToolTip || string.IsNullOrEmpty(vm.Content)) return null;

            try
            {
                var title = await webMetadataService.GetPageTitleAsync(vm.Content);
                if (!token.IsCancellationRequested)
                {
                    return string.IsNullOrWhiteSpace(title) ? vm.DisplayContent : title;
                }
            }
            catch (Exception ex)
            {
                if (!token.IsCancellationRequested)
                {
                    Core.Configuration.LogManager.Log(ex, "Failed to load page title.");
                    return vm.DisplayContent;
                }
            }

            return null;
        }

        public async Task<(string? properties, string? typeInfo, bool isMissing)> GetFilePropertiesAsync(ClipViewModel vm, CancellationToken token)
        {
            if (!vm.IsFileBased || string.IsNullOrEmpty(vm.Content))
            {
                return (null, null, false);
            }

            string? fileProperties = null;
            string? fileTypeInfo = null;
            bool isMissing = false;

            try
            {
                var path = vm.Content.Trim();
                var sb = new StringBuilder();

                await Task.Run(async () =>
                {
                    if (token.IsCancellationRequested) return;

                    if (Directory.Exists(path))
                    {
                        var dirInfo = new DirectoryInfo(path);
                        sb.AppendLine($"Modified: {dirInfo.LastWriteTime:yyyy-MM-dd HH:mm}");
                        fileTypeInfo = GetFriendlyClipTypeName(vm.ClipType);
                        try
                        {
                            var dirSize = await Task.Run(() => CalculateDirectorySize(dirInfo, token), token);
                            if (token.IsCancellationRequested) return;
                            sb.AppendLine($"Size: {FormatUtils.FormatBytes(dirSize.Size)}");
                            sb.AppendLine($"Contains: {dirSize.FileCount} files, {dirSize.FolderCount} folders");
                        }
                        catch (OperationCanceledException)
                        {
                            return;
                        }
                        catch (Exception)
                        {
                            sb.AppendLine("Size: (access denied)");
                        }
                    }
                    else if (File.Exists(path))
                    {
                        var fileInfo = new FileInfo(path);
                        sb.AppendLine($"Size: {FormatUtils.FormatBytes(fileInfo.Length)}");
                        sb.AppendLine($"Modified: {fileInfo.LastWriteTime:yyyy-MM-dd HH:mm}");
                        fileTypeInfo = $"{fileInfo.Extension.ToLower()} ({GetFriendlyClipTypeName(vm.ClipType)})";

                        if (vm.IsImage)
                        {
                            try
                            {
                                var extension = Path.GetExtension(path).ToLowerInvariant();
                                if (extension == ".jxl")
                                {
                                    using var image = await Core.Services.ImageDecoder.DecodeAsync(path);
                                    if (image != null)
                                    {
                                        sb.AppendLine($"Dimensions: {image.Width} x {image.Height}");
                                    }
                                }
                                else
                                {
                                    var imageInfo = await Image.IdentifyAsync(path, token);
                                    if (imageInfo != null)
                                    {
                                        sb.AppendLine($"Dimensions: {imageInfo.Width} x {imageInfo.Height}");
                                    }
                                }
                            }
                            catch { /* Ignore */ }
                        }
                    }
                    else
                    {
                        isMissing = true;
                    }

                    if (!token.IsCancellationRequested && !isMissing)
                    {
                        fileProperties = sb.ToString().Trim();
                    }
                }, token);
            }
            catch (Exception ex)
            {
                if (!token.IsCancellationRequested)
                {
                    fileProperties = "Error reading properties.";
                }
                Core.Configuration.LogManager.Log(ex, "Failed to load file properties.");
            }

            return (fileProperties, fileTypeInfo, isMissing);
        }

        private (long Size, int FileCount, int FolderCount) CalculateDirectorySize(DirectoryInfo dirInfo, CancellationToken token)
        {
            long size = 0;
            int fileCount = 0;
            int folderCount = 0;

            try
            {
                foreach (var file in dirInfo.EnumerateFiles())
                {
                    token.ThrowIfCancellationRequested();
                    size += file.Length;
                    fileCount++;
                }

                foreach (var dir in dirInfo.EnumerateDirectories())
                {
                    token.ThrowIfCancellationRequested();
                    var subDirSize = CalculateDirectorySize(dir, token);
                    size += subDirSize.Size;
                    fileCount += subDirSize.FileCount;
                    folderCount += subDirSize.FolderCount + 1; // +1 for the current subdirectory
                }
            }
            catch (UnauthorizedAccessException) { /* ignore */ }

            return (size, fileCount, folderCount);
        }

        private string GetFriendlyClipTypeName(string clipType)
        {
            return clipType switch
            {
                AppConstants.ClipTypes.Archive => "Archive File",
                AppConstants.ClipTypes.Audio => "Audio File",
                AppConstants.ClipTypes.Dev => "Dev File",
                AppConstants.ClipTypes.Danger => "Potentially Unsafe File",
                AppConstants.ClipTypes.Database => "Database File",
                AppConstants.ClipTypes.Document => "Document File",
                AppConstants.ClipTypes.FileLink => "Link File",
                AppConstants.ClipTypes.FileText => "Text File",
                AppConstants.ClipTypes.Folder => "Folder",
                AppConstants.ClipTypes.Font => "Font File",
                AppConstants.ClipTypes.Generic => "Generic File",
                AppConstants.ClipTypes.Image => "Image File",
                AppConstants.ClipTypes.System => "System File",
                AppConstants.ClipTypes.Video => "Video File",
                _ => "File"
            };
        }
    }
}