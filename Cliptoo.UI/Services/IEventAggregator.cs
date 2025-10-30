namespace Cliptoo.UI.Services
{
    public interface IEventAggregator
    {
        void Publish<TMessage>(TMessage message) where TMessage : class;
        void Subscribe<TMessage>(Action<TMessage> handler) where TMessage : class;
    }

}