namespace Cliptoo.UI.Services
{
    public interface IDialogService
    {
        string? ShowOpenFileDialog(string title, string filter);
        string? ShowSaveFileDialog(string title, string filter, string initialDirectory, string fileName);
    }
}