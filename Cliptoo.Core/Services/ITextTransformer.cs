namespace Cliptoo.Core.Services
{
    public interface ITextTransformer
    {
        string Transform(string content, string transformType);
    }
}