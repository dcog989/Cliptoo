using System.Collections.Concurrent;

namespace Cliptoo.UI.Services
{
    public class EventAggregator : IEventAggregator
    {
        private readonly ConcurrentDictionary<Type, List<Action<object>>> _subscriptions = new();

        public void Publish<TMessage>(TMessage message) where TMessage : class
        {
            if (_subscriptions.TryGetValue(typeof(TMessage), out var handlers))
            {
                foreach (var handler in handlers.ToList()) // ToList creates a copy, safe for modification
                {
                    handler(message);
                }
            }
        }

        public void Subscribe<TMessage>(Action<TMessage> handler) where TMessage : class
        {
            var handlers = _subscriptions.GetOrAdd(typeof(TMessage), _ => new List<Action<object>>());
            handlers.Add(msg => handler((TMessage)msg));
        }

    }
}