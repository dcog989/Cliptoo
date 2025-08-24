namespace Cliptoo.Core.Services
{
    public interface ICompareToolService
    {
        (string? Path, string? Args) FindCompareTool();
        string GetArgsForPath(string toolPath);
    }
}