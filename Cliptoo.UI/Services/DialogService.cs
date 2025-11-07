using Microsoft.Win32;

namespace Cliptoo.UI.Services
{
    internal class DialogService : IDialogService
    {
        public string? ShowOpenFileDialog(string title, string filter)
        {
            var openFileDialog = new OpenFileDialog
            {
                Filter = filter,
                Title = title
            };
            return openFileDialog.ShowDialog() == true ? openFileDialog.FileName : null;
        }

        public string? ShowSaveFileDialog(string title, string filter, string initialDirectory, string fileName)
        {
            var saveFileDialog = new SaveFileDialog
            {
                Filter = filter,
                Title = title,
                InitialDirectory = initialDirectory,
                FileName = fileName
            };
            return saveFileDialog.ShowDialog() == true ? saveFileDialog.FileName : null;
        }
    }
}