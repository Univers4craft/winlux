// BCL Winlux : Queue<T>, Stack<T>, HashSet<T>, LinkedList<T> et
// ConcurrentDictionary<TKey, TValue> (#1952). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« Queue<T> Class »,
// « Stack<T> Class », « HashSet<T> Class », « LinkedList<T> Class »,
// « ConcurrentDictionary<TKey,TValue> Class ») ; aucun code de Microsoft,
// de Mono ou de Wine.
//
// Sous .NET Framework, Queue, Stack et LinkedList sont rangés dans
// System.dll et HashSet dans System.Core.dll : le moteur sert depuis notre
// mscorlib les types de ces assemblies du Framework (#1947).

using System.Collections;
using System.Collections.Generic;

namespace System.Collections.Generic
{
    // File circulaire : _head est le prochain élément sorti, _tail la
    // prochaine place libre.
    public class Queue<T> : IEnumerable<T>, ICollection, IReadOnlyCollection<T>
    {
        private T[] _array;
        private int _head;
        private int _tail;
        private int _size;
        private int _version;

        public Queue()
        {
            _array = new T[0];
        }

        public Queue(int capacity)
        {
            if (capacity < 0)
                throw new ArgumentOutOfRangeException("capacity");
            _array = new T[capacity];
        }

        public Queue(IEnumerable<T> collection)
        {
            if (collection == null)
                throw new ArgumentNullException("collection");
            _array = new T[4];
            foreach (T item in collection)
                Enqueue(item);
        }

        public int Count { get { return _size; } }

        public void Clear()
        {
            for (int i = 0; i < _array.Length; i++)
                _array[i] = default(T);
            _head = 0;
            _tail = 0;
            _size = 0;
            _version++;
        }

        public void Enqueue(T item)
        {
            if (_size == _array.Length)
                SetCapacity(_array.Length < 4 ? 4 : _array.Length * 2);
            _array[_tail] = item;
            _tail = (_tail + 1) % _array.Length;
            _size++;
            _version++;
        }

        public T Dequeue()
        {
            if (_size == 0)
                throw new InvalidOperationException("Queue empty.");
            T item = _array[_head];
            _array[_head] = default(T);
            _head = (_head + 1) % _array.Length;
            _size--;
            _version++;
            return item;
        }

        public bool TryDequeue(out T result)
        {
            if (_size == 0)
            {
                result = default(T);
                return false;
            }
            result = Dequeue();
            return true;
        }

        public T Peek()
        {
            if (_size == 0)
                throw new InvalidOperationException("Queue empty.");
            return _array[_head];
        }

        public bool TryPeek(out T result)
        {
            if (_size == 0)
            {
                result = default(T);
                return false;
            }
            result = _array[_head];
            return true;
        }

        public bool Contains(T item)
        {
            EqualityComparer<T> c = EqualityComparer<T>.Default;
            for (int i = 0; i < _size; i++)
            {
                if (c.Equals(At(i), item))
                    return true;
            }
            return false;
        }

        public T[] ToArray()
        {
            T[] result = new T[_size];
            for (int i = 0; i < _size; i++)
                result[i] = At(i);
            return result;
        }

        public void CopyTo(T[] array, int arrayIndex)
        {
            if (array == null)
                throw new ArgumentNullException("array");
            if (arrayIndex < 0 || array.Length - arrayIndex < _size)
                throw new ArgumentOutOfRangeException("arrayIndex");
            for (int i = 0; i < _size; i++)
                array[arrayIndex + i] = At(i);
        }

        public void TrimExcess()
        {
            if (_size < _array.Length * 9 / 10)
                SetCapacity(_size);
        }

        internal T At(int i)
        {
            return _array[(_head + i) % _array.Length];
        }

        private void SetCapacity(int capacity)
        {
            T[] array = new T[capacity];
            for (int i = 0; i < _size; i++)
                array[i] = At(i);
            _array = array;
            _head = 0;
            _tail = capacity == 0 ? 0 : _size % capacity;
            _version++;
        }

        public Enumerator GetEnumerator() { return new Enumerator(this); }
        IEnumerator<T> IEnumerable<T>.GetEnumerator() { return new Enumerator(this); }
        IEnumerator IEnumerable.GetEnumerator() { return new Enumerator(this); }

        public struct Enumerator : IEnumerator<T>
        {
            private readonly Queue<T> _queue;
            private readonly int _version;
            private int _index;
            private T _current;

            internal Enumerator(Queue<T> queue)
            {
                _queue = queue;
                _version = queue._version;
                _index = 0;
                _current = default(T);
            }

            public bool MoveNext()
            {
                if (_version != _queue._version)
                    throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                if (_index < _queue._size)
                {
                    _current = _queue.At(_index);
                    _index++;
                    return true;
                }
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

    public class Stack<T> : IEnumerable<T>, ICollection, IReadOnlyCollection<T>
    {
        private T[] _array;
        private int _size;
        private int _version;

        public Stack()
        {
            _array = new T[0];
        }

        public Stack(int capacity)
        {
            if (capacity < 0)
                throw new ArgumentOutOfRangeException("capacity");
            _array = new T[capacity];
        }

        public Stack(IEnumerable<T> collection)
        {
            if (collection == null)
                throw new ArgumentNullException("collection");
            _array = new T[4];
            foreach (T item in collection)
                Push(item);
        }

        public int Count { get { return _size; } }

        public void Clear()
        {
            for (int i = 0; i < _size; i++)
                _array[i] = default(T);
            _size = 0;
            _version++;
        }

        public void Push(T item)
        {
            if (_size == _array.Length)
            {
                T[] array = new T[_array.Length < 4 ? 4 : _array.Length * 2];
                for (int i = 0; i < _size; i++)
                    array[i] = _array[i];
                _array = array;
            }
            _array[_size] = item;
            _size++;
            _version++;
        }

        public T Pop()
        {
            if (_size == 0)
                throw new InvalidOperationException("Stack empty.");
            _size--;
            T item = _array[_size];
            _array[_size] = default(T);
            _version++;
            return item;
        }

        public bool TryPop(out T result)
        {
            if (_size == 0)
            {
                result = default(T);
                return false;
            }
            result = Pop();
            return true;
        }

        public T Peek()
        {
            if (_size == 0)
                throw new InvalidOperationException("Stack empty.");
            return _array[_size - 1];
        }

        public bool TryPeek(out T result)
        {
            if (_size == 0)
            {
                result = default(T);
                return false;
            }
            result = _array[_size - 1];
            return true;
        }

        public bool Contains(T item)
        {
            EqualityComparer<T> c = EqualityComparer<T>.Default;
            for (int i = 0; i < _size; i++)
            {
                if (c.Equals(_array[i], item))
                    return true;
            }
            return false;
        }

        // Ordre de sortie : le dernier empilé en premier.
        public T[] ToArray()
        {
            T[] result = new T[_size];
            for (int i = 0; i < _size; i++)
                result[i] = _array[_size - 1 - i];
            return result;
        }

        public void CopyTo(T[] array, int arrayIndex)
        {
            if (array == null)
                throw new ArgumentNullException("array");
            if (arrayIndex < 0 || array.Length - arrayIndex < _size)
                throw new ArgumentOutOfRangeException("arrayIndex");
            for (int i = 0; i < _size; i++)
                array[arrayIndex + i] = _array[_size - 1 - i];
        }

        public void TrimExcess() { }

        public Enumerator GetEnumerator() { return new Enumerator(this); }
        IEnumerator<T> IEnumerable<T>.GetEnumerator() { return new Enumerator(this); }
        IEnumerator IEnumerable.GetEnumerator() { return new Enumerator(this); }

        public struct Enumerator : IEnumerator<T>
        {
            private readonly Stack<T> _stack;
            private readonly int _version;
            private int _index;
            private T _current;

            internal Enumerator(Stack<T> stack)
            {
                _stack = stack;
                _version = stack._version;
                _index = stack._size;
                _current = default(T);
            }

            public bool MoveNext()
            {
                if (_version != _stack._version)
                    throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                if (_index > 0)
                {
                    _index--;
                    _current = _stack._array[_index];
                    return true;
                }
                _current = default(T);
                return false;
            }

            public T Current { get { return _current; } }
            object IEnumerator.Current { get { return _current; } }

            void IEnumerator.Reset()
            {
                _index = _stack._size;
                _current = default(T);
            }

            public void Dispose() { }
        }
    }

    // Ensemble à chaînage par indices, comme Dictionary ; null est un
    // élément permis.
    public class HashSet<T> : ICollection<T>, IReadOnlyCollection<T>
    {
        private int[] _buckets;
        private int[] _next;
        private int[] _hashes;
        private T[] _slots;
        private int _count;
        private int _freeList;
        private int _freeCount;
        private int _version;
        private readonly IEqualityComparer<T> _comparer;

        public HashSet() : this((IEqualityComparer<T>)null) { }

        public HashSet(IEqualityComparer<T> comparer)
        {
            _comparer = comparer != null ? comparer : EqualityComparer<T>.Default;
            Initialize(3);
        }

        public HashSet(int capacity) : this(capacity, null) { }

        public HashSet(int capacity, IEqualityComparer<T> comparer)
        {
            if (capacity < 0)
                throw new ArgumentOutOfRangeException("capacity");
            _comparer = comparer != null ? comparer : EqualityComparer<T>.Default;
            Initialize(capacity < 3 ? 3 : capacity);
        }

        public HashSet(IEnumerable<T> collection) : this(collection, null) { }

        public HashSet(IEnumerable<T> collection, IEqualityComparer<T> comparer) : this(comparer)
        {
            if (collection == null)
                throw new ArgumentNullException("collection");
            UnionWith(collection);
        }

        private void Initialize(int capacity)
        {
            _buckets = new int[capacity];
            for (int i = 0; i < capacity; i++)
                _buckets[i] = -1;
            _next = new int[capacity];
            _hashes = new int[capacity];
            _slots = new T[capacity];
            _count = 0;
            _freeList = -1;
            _freeCount = 0;
        }

        public IEqualityComparer<T> Comparer { get { return _comparer; } }
        public int Count { get { return _count - _freeCount; } }
        bool ICollection<T>.IsReadOnly { get { return false; } }

        private int Hash(T item)
        {
            return item == null ? 0 : _comparer.GetHashCode(item) & 0x7FFFFFFF;
        }

        private int Find(T item)
        {
            int h = Hash(item);
            for (int i = _buckets[h % _buckets.Length]; i >= 0; i = _next[i])
            {
                if (_hashes[i] == h && _comparer.Equals(_slots[i], item))
                    return i;
            }
            return -1;
        }

        public bool Contains(T item)
        {
            return Find(item) >= 0;
        }

        public bool Add(T item)
        {
            if (Find(item) >= 0)
                return false;
            int h = Hash(item);
            int index;
            if (_freeCount > 0)
            {
                index = _freeList;
                _freeList = _next[index];
                _freeCount--;
            }
            else
            {
                if (_count == _slots.Length)
                    Grow();
                index = _count;
                _count++;
            }
            int bucket = h % _buckets.Length;
            _hashes[index] = h;
            _slots[index] = item;
            _next[index] = _buckets[bucket];
            _buckets[bucket] = index;
            _version++;
            return true;
        }

        void ICollection<T>.Add(T item)
        {
            Add(item);
        }

        private void Grow()
        {
            int size = _slots.Length * 2 + 1;
            int[] buckets = new int[size];
            for (int i = 0; i < size; i++)
                buckets[i] = -1;
            int[] next = new int[size];
            int[] hashes = new int[size];
            T[] slots = new T[size];
            for (int i = 0; i < _count; i++)
            {
                hashes[i] = _hashes[i];
                slots[i] = _slots[i];
            }
            // Les places libérées (_hashes = -1) restent hors des chaînes.
            for (int i = 0; i < _count; i++)
            {
                if (hashes[i] < 0)
                {
                    next[i] = _next[i];
                    continue;
                }
                int bucket = hashes[i] % size;
                next[i] = buckets[bucket];
                buckets[bucket] = i;
            }
            _buckets = buckets;
            _next = next;
            _hashes = hashes;
            _slots = slots;
        }

        public bool Remove(T item)
        {
            int h = Hash(item);
            int bucket = h % _buckets.Length;
            int last = -1;
            for (int i = _buckets[bucket]; i >= 0; last = i, i = _next[i])
            {
                if (_hashes[i] == h && _comparer.Equals(_slots[i], item))
                {
                    if (last < 0)
                        _buckets[bucket] = _next[i];
                    else
                        _next[last] = _next[i];
                    _hashes[i] = -1;
                    _slots[i] = default(T);
                    _next[i] = _freeList;
                    _freeList = i;
                    _freeCount++;
                    _version++;
                    return true;
                }
            }
            return false;
        }

        public int RemoveWhere(Predicate<T> match)
        {
            if (match == null)
                throw new ArgumentNullException("match");
            List<T> doomed = new List<T>();
            for (int i = 0; i < _count; i++)
            {
                if (_hashes[i] >= 0 && match(_slots[i]))
                    doomed.Add(_slots[i]);
            }
            for (int i = 0; i < doomed.Count; i++)
                Remove(doomed[i]);
            return doomed.Count;
        }

        public void Clear()
        {
            Initialize(_buckets.Length);
            _version++;
        }

        public void UnionWith(IEnumerable<T> other)
        {
            if (other == null)
                throw new ArgumentNullException("other");
            foreach (T item in other)
                Add(item);
        }

        public void ExceptWith(IEnumerable<T> other)
        {
            if (other == null)
                throw new ArgumentNullException("other");
            foreach (T item in other)
                Remove(item);
        }

        public void IntersectWith(IEnumerable<T> other)
        {
            if (other == null)
                throw new ArgumentNullException("other");
            HashSet<T> keep = new HashSet<T>(other, _comparer);
            RemoveWhere(x => !keep.Contains(x));
        }

        public bool Overlaps(IEnumerable<T> other)
        {
            if (other == null)
                throw new ArgumentNullException("other");
            foreach (T item in other)
            {
                if (Contains(item))
                    return true;
            }
            return false;
        }

        public bool IsSubsetOf(IEnumerable<T> other)
        {
            if (other == null)
                throw new ArgumentNullException("other");
            HashSet<T> set = new HashSet<T>(other, _comparer);
            foreach (T item in this)
            {
                if (!set.Contains(item))
                    return false;
            }
            return true;
        }

        public bool IsSupersetOf(IEnumerable<T> other)
        {
            if (other == null)
                throw new ArgumentNullException("other");
            foreach (T item in other)
            {
                if (!Contains(item))
                    return false;
            }
            return true;
        }

        public bool SetEquals(IEnumerable<T> other)
        {
            if (other == null)
                throw new ArgumentNullException("other");
            HashSet<T> set = new HashSet<T>(other, _comparer);
            return set.Count == Count && IsSupersetOf(set);
        }

        public void CopyTo(T[] array)
        {
            CopyTo(array, 0);
        }

        public void CopyTo(T[] array, int arrayIndex)
        {
            if (array == null)
                throw new ArgumentNullException("array");
            if (arrayIndex < 0 || array.Length - arrayIndex < Count)
                throw new ArgumentOutOfRangeException("arrayIndex");
            int j = arrayIndex;
            for (int i = 0; i < _count; i++)
            {
                if (_hashes[i] >= 0)
                    array[j++] = _slots[i];
            }
        }

        public void TrimExcess() { }

        public Enumerator GetEnumerator() { return new Enumerator(this); }
        IEnumerator<T> IEnumerable<T>.GetEnumerator() { return new Enumerator(this); }
        IEnumerator IEnumerable.GetEnumerator() { return new Enumerator(this); }

        public struct Enumerator : IEnumerator<T>
        {
            private readonly HashSet<T> _set;
            private readonly int _version;
            private int _index;
            private T _current;

            internal Enumerator(HashSet<T> set)
            {
                _set = set;
                _version = set._version;
                _index = 0;
                _current = default(T);
            }

            public bool MoveNext()
            {
                if (_version != _set._version)
                    throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                while (_index < _set._count)
                {
                    int i = _index;
                    _index++;
                    if (_set._hashes[i] >= 0)
                    {
                        _current = _set._slots[i];
                        return true;
                    }
                }
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

    public sealed class LinkedListNode<T>
    {
        internal LinkedList<T> _list;
        internal LinkedListNode<T> _next;
        internal LinkedListNode<T> _prev;
        internal T _item;

        public LinkedListNode(T value)
        {
            _item = value;
        }

        public LinkedList<T> List { get { return _list; } }
        public LinkedListNode<T> Next { get { return _next; } }
        public LinkedListNode<T> Previous { get { return _prev; } }
        public T Value { get { return _item; } set { _item = value; } }
    }

    // Liste doublement chaînée ouverte : First.Previous et Last.Next valent
    // null.
    public class LinkedList<T> : ICollection<T>, IReadOnlyCollection<T>
    {
        private LinkedListNode<T> _first;
        private LinkedListNode<T> _last;
        private int _count;
        private int _version;

        public LinkedList() { }

        public LinkedList(IEnumerable<T> collection)
        {
            if (collection == null)
                throw new ArgumentNullException("collection");
            foreach (T item in collection)
                AddLast(item);
        }

        public int Count { get { return _count; } }
        public LinkedListNode<T> First { get { return _first; } }
        public LinkedListNode<T> Last { get { return _last; } }
        bool ICollection<T>.IsReadOnly { get { return false; } }

        private void Check(LinkedListNode<T> node)
        {
            if (node == null)
                throw new ArgumentNullException("node");
            if (node._list != this)
                throw new InvalidOperationException("The LinkedList node does not belong to current LinkedList.");
        }

        private static void CheckNew(LinkedListNode<T> node)
        {
            if (node == null)
                throw new ArgumentNullException("node");
            if (node._list != null)
                throw new InvalidOperationException("The LinkedList node belongs a LinkedList.");
        }

        // Range `node` juste avant `before` (null : en fin de liste).
        private void Link(LinkedListNode<T> node, LinkedListNode<T> before)
        {
            node._list = this;
            if (before == null)
            {
                node._prev = _last;
                node._next = null;
                if (_last != null)
                    _last._next = node;
                else
                    _first = node;
                _last = node;
            }
            else
            {
                node._next = before;
                node._prev = before._prev;
                if (before._prev != null)
                    before._prev._next = node;
                else
                    _first = node;
                before._prev = node;
            }
            _count++;
            _version++;
        }

        public LinkedListNode<T> AddFirst(T value)
        {
            LinkedListNode<T> node = new LinkedListNode<T>(value);
            Link(node, _first);
            return node;
        }

        public void AddFirst(LinkedListNode<T> node)
        {
            CheckNew(node);
            Link(node, _first);
        }

        public LinkedListNode<T> AddLast(T value)
        {
            LinkedListNode<T> node = new LinkedListNode<T>(value);
            Link(node, null);
            return node;
        }

        public void AddLast(LinkedListNode<T> node)
        {
            CheckNew(node);
            Link(node, null);
        }

        void ICollection<T>.Add(T value)
        {
            AddLast(value);
        }

        public LinkedListNode<T> AddBefore(LinkedListNode<T> node, T value)
        {
            Check(node);
            LinkedListNode<T> created = new LinkedListNode<T>(value);
            Link(created, node);
            return created;
        }

        public LinkedListNode<T> AddAfter(LinkedListNode<T> node, T value)
        {
            Check(node);
            LinkedListNode<T> created = new LinkedListNode<T>(value);
            Link(created, node._next);
            return created;
        }

        public void Remove(LinkedListNode<T> node)
        {
            Check(node);
            if (node._prev != null)
                node._prev._next = node._next;
            else
                _first = node._next;
            if (node._next != null)
                node._next._prev = node._prev;
            else
                _last = node._prev;
            node._list = null;
            node._next = null;
            node._prev = null;
            _count--;
            _version++;
        }

        public bool Remove(T value)
        {
            LinkedListNode<T> node = Find(value);
            if (node == null)
                return false;
            Remove(node);
            return true;
        }

        public void RemoveFirst()
        {
            if (_first == null)
                throw new InvalidOperationException("The LinkedList is empty.");
            Remove(_first);
        }

        public void RemoveLast()
        {
            if (_last == null)
                throw new InvalidOperationException("The LinkedList is empty.");
            Remove(_last);
        }

        public LinkedListNode<T> Find(T value)
        {
            EqualityComparer<T> c = EqualityComparer<T>.Default;
            for (LinkedListNode<T> n = _first; n != null; n = n._next)
            {
                if (c.Equals(n._item, value))
                    return n;
            }
            return null;
        }

        public LinkedListNode<T> FindLast(T value)
        {
            EqualityComparer<T> c = EqualityComparer<T>.Default;
            for (LinkedListNode<T> n = _last; n != null; n = n._prev)
            {
                if (c.Equals(n._item, value))
                    return n;
            }
            return null;
        }

        public bool Contains(T value)
        {
            return Find(value) != null;
        }

        public void Clear()
        {
            LinkedListNode<T> n = _first;
            while (n != null)
            {
                LinkedListNode<T> next = n._next;
                n._list = null;
                n._next = null;
                n._prev = null;
                n = next;
            }
            _first = null;
            _last = null;
            _count = 0;
            _version++;
        }

        public void CopyTo(T[] array, int index)
        {
            if (array == null)
                throw new ArgumentNullException("array");
            if (index < 0 || array.Length - index < _count)
                throw new ArgumentOutOfRangeException("index");
            for (LinkedListNode<T> n = _first; n != null; n = n._next)
                array[index++] = n._item;
        }

        public Enumerator GetEnumerator() { return new Enumerator(this); }
        IEnumerator<T> IEnumerable<T>.GetEnumerator() { return new Enumerator(this); }
        IEnumerator IEnumerable.GetEnumerator() { return new Enumerator(this); }

        public struct Enumerator : IEnumerator<T>
        {
            private readonly LinkedList<T> _list;
            private readonly int _version;
            private LinkedListNode<T> _node;
            private T _current;
            private bool _started;

            internal Enumerator(LinkedList<T> list)
            {
                _list = list;
                _version = list._version;
                _node = null;
                _current = default(T);
                _started = false;
            }

            public bool MoveNext()
            {
                if (_version != _list._version)
                    throw new InvalidOperationException("Collection was modified; enumeration operation may not execute.");
                _node = _started ? (_node == null ? null : _node._next) : _list._first;
                _started = true;
                if (_node == null)
                {
                    _current = default(T);
                    return false;
                }
                _current = _node._item;
                return true;
            }

            public T Current { get { return _current; } }
            object IEnumerator.Current { get { return _current; } }

            void IEnumerator.Reset()
            {
                _node = null;
                _started = false;
                _current = default(T);
            }

            public void Dispose() { }
        }
    }
}

namespace System.Collections.Concurrent
{
    // Dictionnaire protégé par un verrou unique. Les fabriques passées à
    // GetOrAdd et AddOrUpdate s'exécutent hors du verrou, comme le documente
    // Microsoft : elles peuvent donc tourner plusieurs fois pour une clé.
    // L'énumération porte sur une copie prise sous le verrou.
    public class ConcurrentDictionary<TKey, TValue> : IDictionary<TKey, TValue>, IReadOnlyDictionary<TKey, TValue>
    {
        private readonly Dictionary<TKey, TValue> _dict;
        private readonly object _lock = new object();

        public ConcurrentDictionary()
        {
            _dict = new Dictionary<TKey, TValue>();
        }

        public ConcurrentDictionary(IEqualityComparer<TKey> comparer)
        {
            _dict = new Dictionary<TKey, TValue>(comparer);
        }

        public ConcurrentDictionary(int concurrencyLevel, int capacity)
            : this(concurrencyLevel, capacity, null) { }

        public ConcurrentDictionary(int concurrencyLevel, int capacity, IEqualityComparer<TKey> comparer)
        {
            if (concurrencyLevel < 1)
                throw new ArgumentOutOfRangeException("concurrencyLevel");
            _dict = new Dictionary<TKey, TValue>(capacity, comparer);
        }

        public ConcurrentDictionary(IEnumerable<KeyValuePair<TKey, TValue>> collection)
            : this(collection, null) { }

        public ConcurrentDictionary(IEnumerable<KeyValuePair<TKey, TValue>> collection, IEqualityComparer<TKey> comparer)
        {
            if (collection == null)
                throw new ArgumentNullException("collection");
            _dict = new Dictionary<TKey, TValue>(comparer);
            foreach (KeyValuePair<TKey, TValue> pair in collection)
            {
                if (!TryAdd(pair.Key, pair.Value))
                    throw new ArgumentException("The source argument contains duplicate keys.");
            }
        }

        private static void CheckKey(TKey key)
        {
            if (key == null)
                throw new ArgumentNullException("key");
        }

        public int Count
        {
            get { lock (_lock) { return _dict.Count; } }
        }

        public bool IsEmpty
        {
            get { lock (_lock) { return _dict.Count == 0; } }
        }

        bool ICollection<KeyValuePair<TKey, TValue>>.IsReadOnly { get { return false; } }

        public TValue this[TKey key]
        {
            get
            {
                CheckKey(key);
                TValue value;
                if (!TryGetValue(key, out value))
                    throw new KeyNotFoundException(string.Concat("The given key '", key.ToString(), "' was not present in the dictionary."));
                return value;
            }
            set
            {
                CheckKey(key);
                lock (_lock) { _dict[key] = value; }
            }
        }

        public bool ContainsKey(TKey key)
        {
            CheckKey(key);
            lock (_lock) { return _dict.ContainsKey(key); }
        }

        public bool TryGetValue(TKey key, out TValue value)
        {
            CheckKey(key);
            lock (_lock) { return _dict.TryGetValue(key, out value); }
        }

        public bool TryAdd(TKey key, TValue value)
        {
            CheckKey(key);
            lock (_lock)
            {
                if (_dict.ContainsKey(key))
                    return false;
                _dict.Add(key, value);
                return true;
            }
        }

        public bool TryRemove(TKey key, out TValue value)
        {
            CheckKey(key);
            lock (_lock)
            {
                if (!_dict.TryGetValue(key, out value))
                    return false;
                _dict.Remove(key);
                return true;
            }
        }

        public bool TryUpdate(TKey key, TValue newValue, TValue comparisonValue)
        {
            CheckKey(key);
            lock (_lock)
            {
                TValue current;
                if (!_dict.TryGetValue(key, out current))
                    return false;
                if (!EqualityComparer<TValue>.Default.Equals(current, comparisonValue))
                    return false;
                _dict[key] = newValue;
                return true;
            }
        }

        public TValue GetOrAdd(TKey key, TValue value)
        {
            CheckKey(key);
            lock (_lock)
            {
                TValue current;
                if (_dict.TryGetValue(key, out current))
                    return current;
                _dict.Add(key, value);
                return value;
            }
        }

        public TValue GetOrAdd(TKey key, Func<TKey, TValue> valueFactory)
        {
            CheckKey(key);
            if (valueFactory == null)
                throw new ArgumentNullException("valueFactory");
            TValue current;
            if (TryGetValue(key, out current))
                return current;
            return GetOrAdd(key, valueFactory(key));
        }

        public TValue AddOrUpdate(TKey key, TValue addValue, Func<TKey, TValue, TValue> updateValueFactory)
        {
            CheckKey(key);
            if (updateValueFactory == null)
                throw new ArgumentNullException("updateValueFactory");
            while (true)
            {
                TValue current;
                if (TryGetValue(key, out current))
                {
                    TValue updated = updateValueFactory(key, current);
                    if (TryUpdate(key, updated, current))
                        return updated;
                }
                else if (TryAdd(key, addValue))
                {
                    return addValue;
                }
            }
        }

        public TValue AddOrUpdate(TKey key, Func<TKey, TValue> addValueFactory, Func<TKey, TValue, TValue> updateValueFactory)
        {
            CheckKey(key);
            if (addValueFactory == null)
                throw new ArgumentNullException("addValueFactory");
            if (updateValueFactory == null)
                throw new ArgumentNullException("updateValueFactory");
            while (true)
            {
                TValue current;
                if (TryGetValue(key, out current))
                {
                    TValue updated = updateValueFactory(key, current);
                    if (TryUpdate(key, updated, current))
                        return updated;
                }
                else
                {
                    TValue added = addValueFactory(key);
                    if (TryAdd(key, added))
                        return added;
                }
            }
        }

        public void Clear()
        {
            lock (_lock) { _dict.Clear(); }
        }

        public KeyValuePair<TKey, TValue>[] ToArray()
        {
            lock (_lock)
            {
                KeyValuePair<TKey, TValue>[] result = new KeyValuePair<TKey, TValue>[_dict.Count];
                int i = 0;
                foreach (KeyValuePair<TKey, TValue> pair in _dict)
                    result[i++] = pair;
                return result;
            }
        }

        public ICollection<TKey> Keys
        {
            get
            {
                lock (_lock) { return new List<TKey>(_dict.Keys); }
            }
        }

        public ICollection<TValue> Values
        {
            get
            {
                lock (_lock) { return new List<TValue>(_dict.Values); }
            }
        }

        public IEnumerator<KeyValuePair<TKey, TValue>> GetEnumerator()
        {
            KeyValuePair<TKey, TValue>[] items = ToArray();
            List<KeyValuePair<TKey, TValue>> copy = new List<KeyValuePair<TKey, TValue>>(items.Length);
            for (int i = 0; i < items.Length; i++)
                copy.Add(items[i]);
            return copy.GetEnumerator();
        }

        IEnumerator IEnumerable.GetEnumerator()
        {
            return GetEnumerator();
        }

        void IDictionary<TKey, TValue>.Add(TKey key, TValue value)
        {
            if (!TryAdd(key, value))
                throw new ArgumentException("An item with the same key has already been added.");
        }

        bool IDictionary<TKey, TValue>.Remove(TKey key)
        {
            TValue ignored;
            return TryRemove(key, out ignored);
        }

        void ICollection<KeyValuePair<TKey, TValue>>.Add(KeyValuePair<TKey, TValue> item)
        {
            ((IDictionary<TKey, TValue>)this).Add(item.Key, item.Value);
        }

        bool ICollection<KeyValuePair<TKey, TValue>>.Contains(KeyValuePair<TKey, TValue> item)
        {
            TValue value;
            return TryGetValue(item.Key, out value) && EqualityComparer<TValue>.Default.Equals(value, item.Value);
        }

        bool ICollection<KeyValuePair<TKey, TValue>>.Remove(KeyValuePair<TKey, TValue> item)
        {
            lock (_lock)
            {
                TValue value;
                if (!_dict.TryGetValue(item.Key, out value) || !EqualityComparer<TValue>.Default.Equals(value, item.Value))
                    return false;
                _dict.Remove(item.Key);
                return true;
            }
        }

        void ICollection<KeyValuePair<TKey, TValue>>.CopyTo(KeyValuePair<TKey, TValue>[] array, int arrayIndex)
        {
            KeyValuePair<TKey, TValue>[] items = ToArray();
            if (array == null)
                throw new ArgumentNullException("array");
            if (arrayIndex < 0 || array.Length - arrayIndex < items.Length)
                throw new ArgumentOutOfRangeException("arrayIndex");
            for (int i = 0; i < items.Length; i++)
                array[arrayIndex + i] = items[i];
        }
    }
}
