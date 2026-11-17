// BCL Winlux : System.Collections.ObjectModel.Collection<T> et
// KeyedCollection<TKey, TItem> (#2783).
// Licence GPL 3. Écrit d'après la documentation publique de l'API
// (« Collection<T> Class », « KeyedCollection<TKey,TItem> Class ») ; aucun
// code de Microsoft, de Mono ou de Wine.
//
// Newtonsoft.Json en dérive (JPropertyKeyedCollection, JsonPropertyCollection) ;
// sans elles, le SetupDownloader de Paint.NET mourait sur
// ICollection`1::get_Count.

using System.Collections;
using System.Collections.Generic;

namespace System.Collections.ObjectModel
{
    public class Collection<T> : IList<T>, IReadOnlyList<T>, IList
    {
        private readonly IList<T> _items;

        public Collection()
        {
            _items = new List<T>();
        }

        // La liste donnée est enveloppée, pas copiée.
        public Collection(IList<T> list)
        {
            if (list == null) throw new ArgumentNullException("list");
            _items = list;
        }

        public int Count { get { return _items.Count; } }

        protected IList<T> Items { get { return _items; } }

        public T this[int index]
        {
            get { return _items[index]; }
            set
            {
                if (_items.IsReadOnly) throw ReadOnly();
                if (index < 0 || index >= _items.Count) throw new ArgumentOutOfRangeException("index");
                SetItem(index, value);
            }
        }

        public void Add(T item)
        {
            if (_items.IsReadOnly) throw ReadOnly();
            InsertItem(_items.Count, item);
        }

        public void Clear()
        {
            if (_items.IsReadOnly) throw ReadOnly();
            ClearItems();
        }

        public bool Contains(T item) { return _items.Contains(item); }
        public void CopyTo(T[] array, int index) { _items.CopyTo(array, index); }
        public IEnumerator<T> GetEnumerator() { return _items.GetEnumerator(); }
        public int IndexOf(T item) { return _items.IndexOf(item); }

        public void Insert(int index, T item)
        {
            if (_items.IsReadOnly) throw ReadOnly();
            if (index < 0 || index > _items.Count) throw new ArgumentOutOfRangeException("index");
            InsertItem(index, item);
        }

        public bool Remove(T item)
        {
            if (_items.IsReadOnly) throw ReadOnly();
            int index = _items.IndexOf(item);
            if (index < 0) return false;
            RemoveItem(index);
            return true;
        }

        public void RemoveAt(int index)
        {
            if (_items.IsReadOnly) throw ReadOnly();
            if (index < 0 || index >= _items.Count) throw new ArgumentOutOfRangeException("index");
            RemoveItem(index);
        }

        // Points d'extension : toute modification passe par eux.
        protected virtual void ClearItems() { _items.Clear(); }
        protected virtual void InsertItem(int index, T item) { _items.Insert(index, item); }
        protected virtual void RemoveItem(int index) { _items.RemoveAt(index); }
        protected virtual void SetItem(int index, T item) { _items[index] = item; }

        bool ICollection<T>.IsReadOnly { get { return _items.IsReadOnly; } }
        IEnumerator IEnumerable.GetEnumerator() { return _items.GetEnumerator(); }
        int ICollection.Count { get { return _items.Count; } }

        object IList.this[int index]
        {
            get { return _items[index]; }
            set { this[index] = (T)value; }
        }

        int IList.Add(object value)
        {
            Add((T)value);
            return Count - 1;
        }

        private static NotSupportedException ReadOnly() { return new NotSupportedException("Collection is read-only."); }
    }

    public abstract class KeyedCollection<TKey, TItem> : Collection<TItem>
    {
        private readonly IEqualityComparer<TKey> _comparer;
        // -1 : jamais de dictionnaire ; sinon il naît quand le nombre de
        // clés dépasse ce seuil.
        private readonly int _threshold;
        private Dictionary<TKey, TItem> _dict;
        private int _keyCount;

        protected KeyedCollection() : this(null, 0) { }
        protected KeyedCollection(IEqualityComparer<TKey> comparer) : this(comparer, 0) { }

        protected KeyedCollection(IEqualityComparer<TKey> comparer, int dictionaryCreationThreshold)
        {
            if (dictionaryCreationThreshold < -1) throw new ArgumentOutOfRangeException("dictionaryCreationThreshold");
            _comparer = comparer ?? EqualityComparer<TKey>.Default;
            _threshold = dictionaryCreationThreshold;
        }

        public IEqualityComparer<TKey> Comparer { get { return _comparer; } }

        protected IDictionary<TKey, TItem> Dictionary { get { return _dict; } }

        protected abstract TKey GetKeyForItem(TItem item);

        public TItem this[TKey key]
        {
            get
            {
                if (key == null) throw new ArgumentNullException("key");
                TItem item;
                if (Find(key, out item)) return item;
                throw new KeyNotFoundException();
            }
        }

        public bool Contains(TKey key)
        {
            if (key == null) throw new ArgumentNullException("key");
            TItem item;
            return Find(key, out item);
        }

        public bool Remove(TKey key)
        {
            if (key == null) throw new ArgumentNullException("key");
            for (int i = 0; i < Items.Count; i++)
            {
                TKey k = GetKeyForItem(Items[i]);
                if (k != null && _comparer.Equals(k, key))
                {
                    RemoveItem(i);
                    return true;
                }
            }
            return false;
        }

        protected void ChangeItemKey(TItem item, TKey newKey)
        {
            if (!ContainsItem(item)) throw new ArgumentException("The item does not exist in the collection.");
            TKey oldKey = GetKeyForItem(item);
            if (_comparer.Equals(oldKey, newKey)) return;
            if (newKey != null) AddKey(newKey, item);
            if (oldKey != null) RemoveKey(oldKey);
        }

        protected override void ClearItems()
        {
            base.ClearItems();
            if (_dict != null) _dict.Clear();
            _keyCount = 0;
        }

        protected override void InsertItem(int index, TItem item)
        {
            TKey key = GetKeyForItem(item);
            if (key != null) AddKey(key, item);
            base.InsertItem(index, item);
        }

        protected override void RemoveItem(int index)
        {
            TKey key = GetKeyForItem(Items[index]);
            if (key != null) RemoveKey(key);
            base.RemoveItem(index);
        }

        protected override void SetItem(int index, TItem item)
        {
            TKey newKey = GetKeyForItem(item);
            TKey oldKey = GetKeyForItem(Items[index]);
            if (_comparer.Equals(oldKey, newKey))
            {
                if (newKey != null && _dict != null) _dict[newKey] = item;
            }
            else
            {
                if (newKey != null) AddKey(newKey, item);
                if (oldKey != null) RemoveKey(oldKey);
            }
            base.SetItem(index, item);
        }

        private bool Find(TKey key, out TItem item)
        {
            if (_dict != null) return _dict.TryGetValue(key, out item);
            foreach (TItem it in Items)
            {
                TKey k = GetKeyForItem(it);
                if (k != null && _comparer.Equals(k, key))
                {
                    item = it;
                    return true;
                }
            }
            item = default(TItem);
            return false;
        }

        private bool ContainsItem(TItem item)
        {
            EqualityComparer<TItem> eq = EqualityComparer<TItem>.Default;
            foreach (TItem it in Items) if (eq.Equals(it, item)) return true;
            return false;
        }

        private void AddKey(TKey key, TItem item)
        {
            TItem existant;
            if (Find(key, out existant)) throw new ArgumentException("An item with the same key has already been added.");
            if (_dict != null)
            {
                _dict.Add(key, item);
            }
            else if (_threshold != -1 && _keyCount + 1 > _threshold)
            {
                // Seuil franchi : le dictionnaire reprend toutes les clés.
                _dict = new Dictionary<TKey, TItem>(_comparer);
                foreach (TItem it in Items)
                {
                    TKey k = GetKeyForItem(it);
                    if (k != null) _dict.Add(k, it);
                }
                _dict.Add(key, item);
            }
            _keyCount++;
        }

        private void RemoveKey(TKey key)
        {
            if (_dict != null) _dict.Remove(key);
            _keyCount--;
        }
    }
}
