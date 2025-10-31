using System.Collections.Concurrent;
using System.Reflection;

namespace Cliptoo.UI.Services
{
    public class EventAggregator : IEventAggregator
    {
        private class WeakHandler
        {
            private readonly WeakReference? _weakTarget;
            private readonly MethodInfo _method;

            public WeakHandler(Delegate handler)
            {
                ArgumentNullException.ThrowIfNull(handler);
                if (handler.Target != null)
                {
                    _weakTarget = new WeakReference(handler.Target);
                }
                _method = handler.Method;
            }

            public bool IsAlive => _weakTarget == null || _weakTarget.IsAlive;

            public void Invoke(object message)
            {
                object? target = null;
                if (_weakTarget != null)
                {
                    target = _weakTarget.Target;
                    if (target == null)
                    {
                        return; // Target has been garbage collected.
                    }
                }

                // If _weakTarget is null, it's a static method, so target is null, which is correct for MethodInfo.Invoke.
                _method.Invoke(target, new[] { message });
            }
        }

        private readonly ConcurrentDictionary<Type, List<WeakHandler>> _subscriptions = new();

        public void Publish<TMessage>(TMessage message) where TMessage : class
        {
            ArgumentNullException.ThrowIfNull(message);

            if (!_subscriptions.TryGetValue(typeof(TMessage), out var handlers))
            {
                return;
            }

            var liveHandlers = new List<WeakHandler>();
            bool needsCleanup = false;

            lock (handlers)
            {
                // Snapshot live handlers and detect if cleanup is needed.
                foreach (var handler in handlers)
                {
                    if (handler.IsAlive)
                    {
                        liveHandlers.Add(handler);
                    }
                    else
                    {
                        needsCleanup = true;
                    }
                }

                if (needsCleanup)
                {
                    handlers.Clear();
                    handlers.AddRange(liveHandlers);
                }
            }

            // Invoke handlers outside the lock.
            foreach (var handler in liveHandlers)
            {
                // A handler could have died between the snapshot and invocation, but Invoke handles this.
                handler.Invoke(message);
            }
        }

        public void Subscribe<TMessage>(Action<TMessage> handler) where TMessage : class
        {
            ArgumentNullException.ThrowIfNull(handler);

            var messageType = typeof(TMessage);
            var handlers = _subscriptions.GetOrAdd(messageType, _ => new List<WeakHandler>());

            lock (handlers)
            {
                handlers.Add(new WeakHandler(handler));
            }
        }
    }
}