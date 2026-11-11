// BCL Winlux : System.Collections.Generic (List<T>, Dictionary<TKey, TValue>,
// interfaces, EqualityComparer<T>) et délégués Action/Func/Predicate (#1896).
// Licence GPL 3. Écrit d'après la documentation publique de l'API
// (« System.Collections.Generic Namespace ») et ECMA-335 ; aucun code de
// Microsoft, de Mono ou de Wine.
//
// Array.Copy n'est pas encore servi par le moteur : les copies se font
// élément par élément.

using System.Collections;
using System.Collections.Generic;

namespace System
{
    public delegate void Action();
    public delegate void Action<in T>(T obj);
    public delegate void Action<in T1, in T2>(T1 arg1, T2 arg2);
    public delegate TResult Func<in T, out TResult>(T arg);
    public delegate TResult Func<in T1, in T2, out TResult>(T1 arg1, T2 arg2);
    public delegate bool Predicate<in T>(T obj);
    public delegate int Comparison<in T>(T x, T y);

    public interface IEquatable<T>
    {
        bool Equals(T other);
    }

    public interface IComparable<in T>
    {
        int CompareTo(T other);
    }

    public class NotSupportedException : SystemException
    {
        public NotSupportedException() { }
        public NotSupportedException(string message) : base(message) { }
    }
}

namespace System.Collections
{
    public interface ICollection : IEnumerable
    {
        int Count { get; }
    }

    public interface IList : ICollection
    {
        object this[int index] { get; set; }
        int Add(object value);
    }
}

namespace System.Collections.Generic
{
    public interface IEnumerator<out T> : IDisposable, IEnumerator
    {
        new T Current { get; }
    }

    public interface IEnumerable<out T> : IEnumerable
    {
        new IEnumerator<T> GetEnumerator();
    }

    public interface ICollection<T> : IEnumerable<T>
    {
        int Count { get; }
        bool IsReadOnly { get; }
        void Add(T item);
        void Clear();
        bool Contains(T item);
        void CopyTo(T[] array, int arrayIndex);
        bool Remove(T item);
    }

    public interface IList<T> : ICollection<T>
    {
        T this[int index] { get; set; }
        int IndexOf(T item);
        void Insert(int index, T item);
        void RemoveAt(int index);
    }

    public interface IReadOnlyCollection<out T> : IEnumerable<T>
    {
        int Count { get; }
    }

    public interface IReadOnlyList<out T> : IReadOnlyCollection<T>
    {
        T this[int index] { get; }
    }

    public interface IEqualityComparer<in T>
    {
        bool Equals(T x, T y);
        int GetHashCode(T obj);
    }

    public interface IDictionary<TKey, TValue> : ICollection<KeyValuePair<TKey, TValue>>
    {
        TValue this[TKey key] { get; set; }
        ICollection<TKey> Keys { get; }
        ICollection<TValue> Values { get; }
        bool ContainsKey(TKey key);
        void Add(TKey key, TValue value);
        bool Remove(TKey key);
        bool TryGetValue(TKey key, out TValue value);
    }

    public interface IReadOnlyDictionary<TKey, TValue> : IReadOnlyCollection<KeyValuePair<TKey, TValue>>
    {
        TValue this[TKey key] { get; }
        bool ContainsKey(TKey key);
        bool TryGetValue(TKey key, out TValue value);
    }

    public class KeyNotFoundException : SystemException
    {
        public KeyNotFoundException() : base("The given key was not present in the dictionary.") { }
        public KeyNotFoundException(string message) : base(message) { }
    }

    public struct KeyValuePair<TKey, TValue>
    {
        private readonly TKey key;
        private readonly TValue value;

        public KeyValuePair(TKey key, TValue value)
        {
            this.key = key;
            this.value = value;
        }

        public TKey Key { get { return key; } }
        public TValue Value { get { return value; } }
    }

    // Égalité par défaut : IEquatable<T> si le type la fournit, sinon
    // Object.Equals / GetHashCode (que le moteur sert par contenu pour les
    // chaînes et les valeurs encadrées).
    public abstract class EqualityComparer<T> : IEqualityComparer<T>
    {
        private static EqualityComparer<T> _default;

        public static EqualityComparer<T> Default
        {
            get
            {
                if (_default == null)
                    _default = new ObjectEqualityComparer<T>();
                return _default;
            }
        }

        public abstract bool Equals(T x, T y);
        public abstract int GetHashCode(T obj);
    }

    internal sealed class ObjectEqualityComparer<T> : EqualityComparer<T>
    {
        public override bool Equals(T x, T y)
        {
            if (x == null)
                return y == null;
            if (y == null)
                return false;
            return x.Equals(y);
        }

        public override int GetHashCode(T obj)
        {
            return obj == null ? 0 : obj.GetHashCode();
        }
    }

    public class List<T> : IList<T>, IReadOnlyList<T>, IList
    {
        private T[] _items;
        private int _size;
        private int _version;

        public List()
        {
            _items = new T[0];
        }

        public List(int capacity)
        {
            if (capacity < 0)
                throw new ArgumentOutOfRangeException("capacity");
            _items = new T[capacity];
        }

        public List(IEnumerable<T> collection)
        {
            if (collection == null)
                throw new ArgumentNullException("collection");
            _items = new T[0];
            AddRange(collection);
        }

        public int Count { get { return _size; } }

        public int Capacity
        {
            get { return _items.Length; }
            set
            {
                if (value < _size)
                    throw new ArgumentOutOfRangeException("value");
                if (value == _items.Length)
                    return;
                T[] items = new T[value];
                for (int i = 0; i < _size; i++)
                    items[i] = _items[i];
                _items = items;
            }
        }

        public T this[int index]
        {
            get
            {
                if ((uint)index >= (uint)_size)
                    throw new ArgumentOutOfRangeException("index");
                return _items[index];
            }
            set
            {
                if ((uint)index >= (uint)_size)
                    throw new ArgumentOutOfRangeException("index");
                _items[index] = value;
                _version++;
            }
        }

        bool ICollection<T>.IsReadOnly { get { return false; } }

        object IList.this[int index]
        {
            get { return this[index]; }
            set { this[index] = (T)value; }
        }

        private void Grow(int min)
        {
            int capacity = _items.Length == 0 ? 4 : _items.Length * 2;
            if (capacity < min)
                capacity = min;
            Capacity = capacity;
        }

        public void Add(T item)
        {
            if (_size == _items.Length)
                Grow(_size + 1);
            _items[_size] = item;
            _size++;
            _version++;
        }

        int IList.Add(object value)
        {
            Add((T)value);
            return _size - 1;
        }

        public void AddRange(IEnumerable<T> collection)
        {
            if (collection == null)
                throw new ArgumentNullException("collection");
            IEnumerator<T> e = collection.GetEnumerator();
            try
            {
                while (e.MoveNext())
                    Add(e.Current);
            }
            finally
            {
                e.Dispose();
            }
        }

        public void Insert(int index, T item)
        {
            if ((uint)index > (uint)_size)
                throw new ArgumentOutOfRangeException("index");
            if (_size == _items.Length)
                Grow(_size + 1);
            for (int i = _size; i > index; i--)
                _items[i] = _items[i - 1];
            _items[index] = item;
            _size++;
            _version++;
        }

        public int IndexOf(T item)
        {
            EqualityComparer<T> c = EqualityComparer<T>.Default;
            for (int i = 0; i < _size; i++)
            {
                if (c.Equals(_items[i], item))
                    return i;
            }
            return -1;
        }

        public bool Contains(T item)
        {
            return IndexOf(item) >= 0;
        }

        public bool Remove(T item)
        {
            int index = IndexOf(item);
            if (index < 0)
                return false;
            RemoveAt(index);
            return true;
        }

        public void RemoveAt(int index)
        {
            if ((uint)index >= (uint)_size)
                throw new ArgumentOutOfRangeException("index");
            _size--;
            for (int i = index; i < _size; i++)
                _items[i] = _items[i + 1];
            _items[_size] = default(T);
            _version++;
        }

        public int RemoveAll(Predicate<T> match)
        {
            if (match == null)
                throw new ArgumentNullException("match");
            int kept = 0;
            for (int i = 0; i < _size; i++)
            {
                if (!match(_items[i]))
                {
                    _items[kept] = _items[i];
                    kept++;
                }
            }
            int removed = _size - kept;
            for (int i = kept; i < _size; i++)
                _items[i] = default(T);
            _size = kept;
            _version++;
            return removed;
        }

        public void Clear()
        {
            for (int i = 0; i < _size; i++)
                _items[i] = default(T);
            _size = 0;
            _version++;
        }

        public T Find(Predicate<T> match)
        {
            if (match == null)
                throw new ArgumentNullException("match");
            for (int i = 0; i < _size; i++)
            {
                if (match(_items[i]))
                    return _items[i];
            }
            return default(T);
        }

        public int FindIndex(Predicate<T> match)
        {
            if (match == null)
                throw new ArgumentNullException("match");
            for (int i = 0; i < _size; i++)
            {
                if (match(_items[i]))
                    return i;
            }
            return -1;
        }

        public List<T> FindAll(Predicate<T> match)
        {
            if (match == null)
                throw new ArgumentNullException("match");
            List<T> result = new List<T>();
            for (int i = 0; i < _size; i++)
            {
                if (match(_items[i]))
                    result.Add(_items[i]);
            }
            return result;
        }

        public bool Exists(Predicate<T> match)
        {
            return FindIndex(match) >= 0;
        }

        public void ForEach(Action<T> action)
        {
            if (action == null)
                throw new ArgumentNullException("action");
            int version = _version;
            for (int i = 0; i < _size; i++)
            {
                if (version != _version)
                    throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                action(_items[i]);
            }
        }

        public void Reverse()
        {
            for (int i = 0, j = _size - 1; i < j; i++, j--)
            {
                T t = _items[i];
                _items[i] = _items[j];
                _items[j] = t;
            }
            _version++;
        }

        // Tri par insertion, stable : suffisant pour les petites listes des
        // programmes de bureau.
        public void Sort(Comparison<T> comparison)
        {
            if (comparison == null)
                throw new ArgumentNullException("comparison");
            for (int i = 1; i < _size; i++)
            {
                T item = _items[i];
                int j = i - 1;
                while (j >= 0 && comparison(_items[j], item) > 0)
                {
                    _items[j + 1] = _items[j];
                    j--;
                }
                _items[j + 1] = item;
            }
            _version++;
        }

        public T[] ToArray()
        {
            T[] array = new T[_size];
            for (int i = 0; i < _size; i++)
                array[i] = _items[i];
            return array;
        }

        public void CopyTo(T[] array, int arrayIndex)
        {
            if (array == null)
                throw new ArgumentNullException("array");
            for (int i = 0; i < _size; i++)
                array[arrayIndex + i] = _items[i];
        }

        public void CopyTo(T[] array)
        {
            CopyTo(array, 0);
        }

        public Enumerator GetEnumerator()
        {
            return new Enumerator(this);
        }

        IEnumerator<T> IEnumerable<T>.GetEnumerator()
        {
            return new Enumerator(this);
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return new Enumerator(this);
        }

        public struct Enumerator : IEnumerator<T>
        {
            private readonly List<T> _list;
            private readonly int _version;
            private int _index;
            private T _current;

            internal Enumerator(List<T> list)
            {
                _list = list;
                _version = list._version;
                _index = 0;
                _current = default(T);
            }

            public bool MoveNext()
            {
                if (_version != _list._version)
                    throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                if (_index < _list._size)
                {
                    _current = _list._items[_index];
                    _index++;
                    return true;
                }
                _index = _list._size + 1;
                _current = default(T);
                return false;
            }

            public T Current { get { return _current; } }

            object IEnumerator.Current { get { return _current; } }

            void IEnumerator.Reset()
            {
                _index = 0;
                _current = default(T);
            }

            public void Dispose() { }
        }
    }

    // Table de hachage à chaînage par indices (buckets → entrées).
    public class Dictionary<TKey, TValue> : IDictionary<TKey, TValue>, IReadOnlyDictionary<TKey, TValue>
    {
        private int[] _buckets;
        private int[] _next;
        private int[] _hashes;
        private TKey[] _keys;
        private TValue[] _values;
        private int _count;
        private int _freeList;
        private int _freeCount;
        private int _version;
        private readonly IEqualityComparer<TKey> _comparer;
        private KeyCollection _keyCollection;
        private ValueCollection _valueCollection;

        public Dictionary() : this(0, null) { }
        public Dictionary(int capacity) : this(capacity, null) { }
        public Dictionary(IEqualityComparer<TKey> comparer) : this(0, comparer) { }

        public Dictionary(int capacity, IEqualityComparer<TKey> comparer)
        {
            if (capacity < 0)
                throw new ArgumentOutOfRangeException("capacity");
            _comparer = comparer != null ? comparer : EqualityComparer<TKey>.Default;
            Initialize(capacity < 3 ? 3 : capacity);
        }

        public Dictionary(IDictionary<TKey, TValue> dictionary) : this(dictionary, null) { }

        public Dictionary(IDictionary<TKey, TValue> dictionary, IEqualityComparer<TKey> comparer)
            : this(dictionary != null ? dictionary.Count : 0, comparer)
        {
            if (dictionary == null)
                throw new ArgumentNullException("dictionary");
            IEnumerator<KeyValuePair<TKey, TValue>> e = dictionary.GetEnumerator();
            try
            {
                while (e.MoveNext())
                    Add(e.Current.Key, e.Current.Value);
            }
            finally
            {
                e.Dispose();
            }
        }

        private void Initialize(int capacity)
        {
            _buckets = new int[capacity];
            for (int i = 0; i < capacity; i++)
                _buckets[i] = -1;
            _next = new int[capacity];
            _hashes = new int[capacity];
            _keys = new TKey[capacity];
            _values = new TValue[capacity];
            _count = 0;
            _freeList = -1;
            _freeCount = 0;
        }

        public IEqualityComparer<TKey> Comparer { get { return _comparer; } }
        public int Count { get { return _count - _freeCount; } }
        bool ICollection<KeyValuePair<TKey, TValue>>.IsReadOnly { get { return false; } }

        private int Hash(TKey key)
        {
            return _comparer.GetHashCode(key) & 0x7FFFFFFF;
        }

        private int FindEntry(TKey key)
        {
            if (key == null)
                throw new ArgumentNullException("key");
            int hash = Hash(key);
            for (int i = _buckets[hash % _buckets.Length]; i >= 0; i = _next[i])
            {
                if (_hashes[i] == hash && _comparer.Equals(_keys[i], key))
                    return i;
            }
            return -1;
        }

        private void Resize()
        {
            int size = _keys.Length * 2 + 1;
            int[] next = new int[size];
            int[] hashes = new int[size];
            TKey[] keys = new TKey[size];
            TValue[] values = new TValue[size];
            for (int i = 0; i < _count; i++)
            {
                next[i] = _next[i];
                hashes[i] = _hashes[i];
                keys[i] = _keys[i];
                values[i] = _values[i];
            }
            int[] buckets = new int[size];
            for (int i = 0; i < size; i++)
                buckets[i] = -1;
            for (int i = 0; i < _count; i++)
            {
                if (hashes[i] >= 0)
                {
                    int b = hashes[i] % size;
                    next[i] = buckets[b];
                    buckets[b] = i;
                }
            }
            _buckets = buckets;
            _next = next;
            _hashes = hashes;
            _keys = keys;
            _values = values;
        }

        private void Insert(TKey key, TValue value, bool add)
        {
            int found = FindEntry(key);
            if (found >= 0)
            {
                if (add)
                    throw new ArgumentException("An item with the same key has already been added.");
                _values[found] = value;
                _version++;
                return;
            }
            int index;
            if (_freeCount > 0)
            {
                index = _freeList;
                _freeList = _next[index];
                _freeCount--;
            }
            else
            {
                if (_count == _keys.Length)
                    Resize();
                index = _count;
                _count++;
            }
            int hash = Hash(key);
            int bucket = hash % _buckets.Length;
            _hashes[index] = hash;
            _keys[index] = key;
            _values[index] = value;
            _next[index] = _buckets[bucket];
            _buckets[bucket] = index;
            _version++;
        }

        public TValue this[TKey key]
        {
            get
            {
                int i = FindEntry(key);
                if (i < 0)
                    throw new KeyNotFoundException();
                return _values[i];
            }
            set { Insert(key, value, false); }
        }

        public void Add(TKey key, TValue value)
        {
            Insert(key, value, true);
        }

        void ICollection<KeyValuePair<TKey, TValue>>.Add(KeyValuePair<TKey, TValue> item)
        {
            Insert(item.Key, item.Value, true);
        }

        public bool ContainsKey(TKey key)
        {
            return FindEntry(key) >= 0;
        }

        public bool ContainsValue(TValue value)
        {
            EqualityComparer<TValue> c = EqualityComparer<TValue>.Default;
            for (int i = 0; i < _count; i++)
            {
                if (_hashes[i] >= 0 && c.Equals(_values[i], value))
                    return true;
            }
            return false;
        }

        bool ICollection<KeyValuePair<TKey, TValue>>.Contains(KeyValuePair<TKey, TValue> item)
        {
            int i = FindEntry(item.Key);
            return i >= 0 && EqualityComparer<TValue>.Default.Equals(_values[i], item.Value);
        }

        public bool TryGetValue(TKey key, out TValue value)
        {
            int i = FindEntry(key);
            if (i >= 0)
            {
                value = _values[i];
                return true;
            }
            value = default(TValue);
            return false;
        }

        public bool Remove(TKey key)
        {
            if (key == null)
                throw new ArgumentNullException("key");
            int hash = Hash(key);
            int bucket = hash % _buckets.Length;
            int last = -1;
            for (int i = _buckets[bucket]; i >= 0; last = i, i = _next[i])
            {
                if (_hashes[i] == hash && _comparer.Equals(_keys[i], key))
                {
                    if (last < 0)
                        _buckets[bucket] = _next[i];
                    else
                        _next[last] = _next[i];
                    _hashes[i] = -1;
                    _keys[i] = default(TKey);
                    _values[i] = default(TValue);
                    _next[i] = _freeList;
                    _freeList = i;
                    _freeCount++;
                    _version++;
                    return true;
                }
            }
            return false;
        }

        bool ICollection<KeyValuePair<TKey, TValue>>.Remove(KeyValuePair<TKey, TValue> item)
        {
            int i = FindEntry(item.Key);
            if (i >= 0 && EqualityComparer<TValue>.Default.Equals(_values[i], item.Value))
                return Remove(item.Key);
            return false;
        }

        public void Clear()
        {
            if (_count > 0)
            {
                Initialize(_keys.Length);
                _version++;
            }
        }

        void ICollection<KeyValuePair<TKey, TValue>>.CopyTo(KeyValuePair<TKey, TValue>[] array, int index)
        {
            if (array == null)
                throw new ArgumentNullException("array");
            for (int i = 0; i < _count; i++)
            {
                if (_hashes[i] >= 0)
                {
                    array[index] = new KeyValuePair<TKey, TValue>(_keys[i], _values[i]);
                    index++;
                }
            }
        }

        public KeyCollection Keys
        {
            get
            {
                if (_keyCollection == null)
                    _keyCollection = new KeyCollection(this);
                return _keyCollection;
            }
        }

        public ValueCollection Values
        {
            get
            {
                if (_valueCollection == null)
                    _valueCollection = new ValueCollection(this);
                return _valueCollection;
            }
        }

        ICollection<TKey> IDictionary<TKey, TValue>.Keys { get { return Keys; } }
        ICollection<TValue> IDictionary<TKey, TValue>.Values { get { return Values; } }

        public Enumerator GetEnumerator()
        {
            return new Enumerator(this);
        }

        IEnumerator<KeyValuePair<TKey, TValue>> IEnumerable<KeyValuePair<TKey, TValue>>.GetEnumerator()
        {
            return new Enumerator(this);
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return new Enumerator(this);
        }

        // Prochaine entrée occupée à partir de `index` (exclu de la liste
        // libre), ou -1.
        internal int NextEntry(int index)
        {
            while (index < _count)
            {
                if (_hashes[index] >= 0)
                    return index;
                index++;
            }
            return -1;
        }

        internal int Version { get { return _version; } }
        internal TKey KeyAt(int index) { return _keys[index]; }
        internal TValue ValueAt(int index) { return _values[index]; }

        public struct Enumerator : IEnumerator<KeyValuePair<TKey, TValue>>
        {
            private readonly Dictionary<TKey, TValue> _dictionary;
            private readonly int _version;
            private int _index;
            private KeyValuePair<TKey, TValue> _current;

            internal Enumerator(Dictionary<TKey, TValue> dictionary)
            {
                _dictionary = dictionary;
                _version = dictionary._version;
                _index = 0;
                _current = new KeyValuePair<TKey, TValue>();
            }

            public bool MoveNext()
            {
                if (_version != _dictionary._version)
                    throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                int i = _dictionary.NextEntry(_index);
                if (i < 0)
                {
                    _index = _dictionary._count + 1;
                    _current = new KeyValuePair<TKey, TValue>();
                    return false;
                }
                _current = new KeyValuePair<TKey, TValue>(_dictionary._keys[i], _dictionary._values[i]);
                _index = i + 1;
                return true;
            }

            public KeyValuePair<TKey, TValue> Current { get { return _current; } }
            object IEnumerator.Current { get { return _current; } }

            void IEnumerator.Reset()
            {
                _index = 0;
                _current = new KeyValuePair<TKey, TValue>();
            }

            public void Dispose() { }
        }

        public sealed class KeyCollection : ICollection<TKey>, IReadOnlyCollection<TKey>
        {
            private readonly Dictionary<TKey, TValue> _dictionary;

            public KeyCollection(Dictionary<TKey, TValue> dictionary)
            {
                if (dictionary == null)
                    throw new ArgumentNullException("dictionary");
                _dictionary = dictionary;
            }

            public int Count { get { return _dictionary.Count; } }
            bool ICollection<TKey>.IsReadOnly { get { return true; } }

            public Enumerator GetEnumerator() { return new Enumerator(_dictionary); }
            IEnumerator<TKey> IEnumerable<TKey>.GetEnumerator() { return new Enumerator(_dictionary); }
            IEnumerator IEnumerable.GetEnumerator() { return new Enumerator(_dictionary); }

            public void CopyTo(TKey[] array, int index)
            {
                if (array == null)
                    throw new ArgumentNullException("array");
                for (int i = _dictionary.NextEntry(0); i >= 0; i = _dictionary.NextEntry(i + 1))
                {
                    array[index] = _dictionary.KeyAt(i);
                    index++;
                }
            }

            bool ICollection<TKey>.Contains(TKey item) { return _dictionary.ContainsKey(item); }
            void ICollection<TKey>.Add(TKey item) { throw new NotSupportedException("Mutating a key collection derived from a dictionary is not allowed."); }
            void ICollection<TKey>.Clear() { throw new NotSupportedException("Mutating a key collection derived from a dictionary is not allowed."); }
            bool ICollection<TKey>.Remove(TKey item) { throw new NotSupportedException("Mutating a key collection derived from a dictionary is not allowed."); }

            public struct Enumerator : IEnumerator<TKey>
            {
                private readonly Dictionary<TKey, TValue> _dictionary;
                private readonly int _version;
                private int _index;
                private TKey _current;

                internal Enumerator(Dictionary<TKey, TValue> dictionary)
                {
                    _dictionary = dictionary;
                    _version = dictionary.Version;
                    _index = 0;
                    _current = default(TKey);
                }

                public bool MoveNext()
                {
                    if (_version != _dictionary.Version)
                        throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                    int i = _dictionary.NextEntry(_index);
                    if (i < 0)
                    {
                        _current = default(TKey);
                        return false;
                    }
                    _current = _dictionary.KeyAt(i);
                    _index = i + 1;
                    return true;
                }

                public TKey Current { get { return _current; } }
                object IEnumerator.Current { get { return _current; } }
                void IEnumerator.Reset() { _index = 0; _current = default(TKey); }
                public void Dispose() { }
            }
        }

        public sealed class ValueCollection : ICollection<TValue>, IReadOnlyCollection<TValue>
        {
            private readonly Dictionary<TKey, TValue> _dictionary;

            public ValueCollection(Dictionary<TKey, TValue> dictionary)
            {
                if (dictionary == null)
                    throw new ArgumentNullException("dictionary");
                _dictionary = dictionary;
            }

            public int Count { get { return _dictionary.Count; } }
            bool ICollection<TValue>.IsReadOnly { get { return true; } }

            public Enumerator GetEnumerator() { return new Enumerator(_dictionary); }
            IEnumerator<TValue> IEnumerable<TValue>.GetEnumerator() { return new Enumerator(_dictionary); }
            IEnumerator IEnumerable.GetEnumerator() { return new Enumerator(_dictionary); }

            public void CopyTo(TValue[] array, int index)
            {
                if (array == null)
                    throw new ArgumentNullException("array");
                for (int i = _dictionary.NextEntry(0); i >= 0; i = _dictionary.NextEntry(i + 1))
                {
                    array[index] = _dictionary.ValueAt(i);
                    index++;
                }
            }

            bool ICollection<TValue>.Contains(TValue item) { return _dictionary.ContainsValue(item); }
            void ICollection<TValue>.Add(TValue item) { throw new NotSupportedException("Mutating a value collection derived from a dictionary is not allowed."); }
            void ICollection<TValue>.Clear() { throw new NotSupportedException("Mutating a value collection derived from a dictionary is not allowed."); }
            bool ICollection<TValue>.Remove(TValue item) { throw new NotSupportedException("Mutating a value collection derived from a dictionary is not allowed."); }

            public struct Enumerator : IEnumerator<TValue>
            {
                private readonly Dictionary<TKey, TValue> _dictionary;
                private readonly int _version;
                private int _index;
                private TValue _current;

                internal Enumerator(Dictionary<TKey, TValue> dictionary)
                {
                    _dictionary = dictionary;
                    _version = dictionary.Version;
                    _index = 0;
                    _current = default(TValue);
                }

                public bool MoveNext()
                {
                    if (_version != _dictionary.Version)
                        throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                    int i = _dictionary.NextEntry(_index);
                    if (i < 0)
                    {
                        _current = default(TValue);
                        return false;
                    }
                    _current = _dictionary.ValueAt(i);
                    _index = i + 1;
                    return true;
                }

                public TValue Current { get { return _current; } }
                object IEnumerator.Current { get { return _current; } }
                void IEnumerator.Reset() { _index = 0; _current = default(TValue); }
                public void Dispose() { }
            }
        }
    }
}
