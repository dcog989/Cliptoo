namespace Cliptoo.Core.Services
{
    public interface ITextTransformation
    {
        string TransformationType { get; }
        string Transform(string content);
    }
}
