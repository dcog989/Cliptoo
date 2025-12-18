using System.Collections.Concurrent;
using System.Linq.Expressions;
using System.Reflection;

namespace Cliptoo.UI.Services
{
    public class EventAggregator : IEventAggregator
    {
        private interface ISubscription
        {
            bool IsAlive { get; }
            void Invoke(object message);
        }

        private class Subscription<TMessage> : ISubscription where TMessage : class
        {
            private readonly WeakReference? _weakTarget;
            private readonly MethodInfo _method;
            private readonly Action<object?, TMessage> _executor;

            public Subscription(Action<TMessage> handler)
            {
                ArgumentNullException.ThrowIfNull(handler);

                if (handler.Target != null)
                {
                    _weakTarget = new WeakReference(handler.Target);
                }

                _method = handler.Method;
                _executor = CreateExecutor(_method);
            }

            public bool IsAlive => _weakTarget == null || _weakTarget.IsAlive;

            public void Invoke(object message)
            {
                if (message is not TMessage typedMessage) return;

                object? target = null;
                if (_weakTarget != null)
                {
                    target = _weakTarget.Target;
                    if (target == null) return;
                }

                _executor(target, typedMessage);
            }

            private static Action<object?, TMessage> CreateExecutor(MethodInfo method)
            {
                var targetParam = Expression.Parameter(typeof(object), "target");
                var messageParam = Expression.Parameter(typeof(TMessage), "message");

                Expression call;
                if (method.IsStatic)
                {
                    call = Expression.Call(method, messageParam);
                }
                else
                {
                    var castTarget = Expression.Convert(targetParam, method.DeclaringType!);
                    call = Expression.Call(castTarget, method, messageParam);
                }

                return Expression.Lambda<Action<object?, TMessage>>(call, targetParam, messageParam).Compile();
            }
        }

        private readonly ConcurrentDictionary<Type, List<ISubscription>> _subscriptions = new();

        public void Publish<TMessage>(TMessage message) where TMessage : class
        {
            ArgumentNullException.ThrowIfNull(message);

            if (!_subscriptions.TryGetValue(typeof(TMessage), out var handlers))
            {
                return;
            }

            List<ISubscription> liveHandlers;
            bool needsCleanup = false;

            lock (handlers)
            {
                liveHandlers = new List<ISubscription>(handlers.Count);
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

            foreach (var handler in liveHandlers)
            {
                handler.Invoke(message);
            }
        }

        public void Subscribe<TMessage>(Action<TMessage> handler) where TMessage : class
        {
            ArgumentNullException.ThrowIfNull(handler);

            var messageType = typeof(TMessage);
            var handlers = _subscriptions.GetOrAdd(messageType, _ => new List<ISubscription>());

            lock (handlers)
            {
                handlers.Add(new Subscription<TMessage>(handler));
            }
        }
    }
}
