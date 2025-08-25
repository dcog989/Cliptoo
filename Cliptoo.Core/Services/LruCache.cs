using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;

namespace Cliptoo.Core.Services
{
    public class LruCache<TKey, TValue> where TKey : notnull
    {
        private readonly int _capacity;
        private readonly Dictionary<TKey, LinkedListNode<LruCacheItem>> _cache;
        private readonly LinkedList<LruCacheItem> _lruList;

        public LruCache(int capacity)
        {
            ArgumentOutOfRangeException.ThrowIfNegativeOrZero(capacity);
            _capacity = capacity;
            _cache = new Dictionary<TKey, LinkedListNode<LruCacheItem>>();
            _lruList = new LinkedList<LruCacheItem>();
        }

        [MethodImpl(MethodImplOptions.Synchronized)]
        public bool TryGetValue(TKey key, out TValue? value)
        {
            if (_cache.TryGetValue(key, out var node))
            {
                value = node.Value.Value;
                _lruList.Remove(node);
                _lruList.AddFirst(node);
                return true;
            }
            value = default;
            return false;
        }

        [MethodImpl(MethodImplOptions.Synchronized)]
        public void Add(TKey key, TValue value)
        {
            if (_cache.TryGetValue(key, out var existingNode))
            {
                _lruList.Remove(existingNode);
                _cache.Remove(key);
            }
            else if (_cache.Count >= _capacity)
            {
                var last = _lruList.Last;
                if (last != null)
                {
                    _cache.Remove(last.Value.Key);
                    _lruList.RemoveLast();
                }
            }

            var cacheItem = new LruCacheItem(key, value);
            var newNode = new LinkedListNode<LruCacheItem>(cacheItem);
            _lruList.AddFirst(newNode);
            _cache[key] = newNode;
        }

        [MethodImpl(MethodImplOptions.Synchronized)]
        public void Remove(TKey key)
        {
            if (_cache.TryGetValue(key, out var node))
            {
                _lruList.Remove(node);
                _cache.Remove(key);
            }
        }

        [MethodImpl(MethodImplOptions.Synchronized)]
        public void Clear()
        {
            _cache.Clear();
            _lruList.Clear();
        }

        private sealed class LruCacheItem
        {
            public TKey Key { get; }
            public TValue Value { get; }

            public LruCacheItem(TKey key, TValue value)
            {
                Key = key;
                Value = value;
            }
        }
    }
}