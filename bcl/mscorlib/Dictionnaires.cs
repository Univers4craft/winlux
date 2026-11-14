// System.Collections : IDictionary, IDictionaryEnumerator, DictionaryEntry
// et Hashtable de la mscorlib Winlux (#2233). Licence GPL 3.
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « IDictionary Interface », « IDictionaryEnumerator », « DictionaryEntry
// Struct », « Hashtable Class ») ; aucun code de Microsoft, de Mono ou de
// Wine.
//
// Environment.GetEnvironmentVariables rend une Hashtable. Elle range ses
// paires dans un Dictionary<object, object>, qui compare les clés par
// Equals et GetHashCode comme une Hashtable sans comparateur.
using System.Collections.Generic;

namespace System.Collections
{
    public struct DictionaryEntry
    {
        private object _key;
        private object _value;

        public DictionaryEntry(object key, object value) { _key = key; _value = value; }
        public object Key { get { return _key; } set { _key = value; } }
        public object Value { get { return _value; } set { _value = value; } }
    }

    public interface IDictionaryEnumerator : IEnumerator
    {
        object Key { get; }
        object Value { get; }
        DictionaryEntry Entry { get; }
    }

    public interface IDictionary : ICollection
    {
        object this[object key] { get; set; }
        ICollection Keys { get; }
        ICollection Values { get; }
        bool IsReadOnly { get; }
        bool IsFixedSize { get; }
        bool Contains(object key);
        void Add(object key, object value);
        void Clear();
        new IDictionaryEnumerator GetEnumerator();
        void Remove(object key);
    }

    public class Hashtable : IDictionary
    {
        private readonly Dictionary<object, object> _paires;

        public Hashtable() { _paires = new Dictionary<object, object>(); }

        public Hashtable(int capacity)
        {
            if (capacity < 0) throw new ArgumentOutOfRangeException("capacity", "Non-negative number required.");
            _paires = new Dictionary<object, object>();
        }

        public Hashtable(IDictionary d) : this()
        {
            if (d == null) throw new ArgumentNullException("d");
            IDictionaryEnumerator e = d.GetEnumerator();
            while (e.MoveNext()) Add(e.Key, e.Value);
        }

        public virtual int Count { get { return _paires.Count; } }
        public virtual bool IsReadOnly { get { return false; } }
        public virtual bool IsFixedSize { get { return false; } }
        public virtual bool IsSynchronized { get { return false; } }
        public virtual object SyncRoot { get { return this; } }

        private static void Cle(object key)
        {
            if (key == null) throw new ArgumentNullException("key");
        }

        // Une clé absente donne null.
        public virtual object this[object key]
        {
            get
            {
                Cle(key);
                object v;
                return _paires.TryGetValue(key, out v) ? v : null;
            }
            set
            {
                Cle(key);
                _paires[key] = value;
            }
        }

        public virtual void Add(object key, object value)
        {
            Cle(key);
            if (_paires.ContainsKey(key))
                throw new ArgumentException("Item has already been added. Key in dictionary: '" + key + "'  Key being added: '" + key + "'");
            _paires.Add(key, value);
        }

        public virtual void Clear() { _paires.Clear(); }
        public virtual bool Contains(object key) { return ContainsKey(key); }

        public virtual bool ContainsKey(object key)
        {
            Cle(key);
            return _paires.ContainsKey(key);
        }

        public virtual bool ContainsValue(object value)
        {
            foreach (KeyValuePair<object, object> p in _paires)
            {
                if (value == null ? p.Value == null : value.Equals(p.Value)) return true;
            }
            return false;
        }

        public virtual void Remove(object key)
        {
            Cle(key);
            _paires.Remove(key);
        }

        public virtual ICollection Keys
        {
            get
            {
                object[] cles = new object[_paires.Count];
                int i = 0;
                foreach (KeyValuePair<object, object> p in _paires) cles[i++] = p.Key;
                return new CollectionFigee(cles);
            }
        }

        public virtual ICollection Values
        {
            get
            {
                object[] valeurs = new object[_paires.Count];
                int i = 0;
                foreach (KeyValuePair<object, object> p in _paires) valeurs[i++] = p.Value;
                return new CollectionFigee(valeurs);
            }
        }

        public virtual object Clone()
        {
            Hashtable copie = new Hashtable();
            foreach (KeyValuePair<object, object> p in _paires) copie._paires.Add(p.Key, p.Value);
            return copie;
        }

        public virtual IDictionaryEnumerator GetEnumerator()
        {
            DictionaryEntry[] entrees = new DictionaryEntry[_paires.Count];
            int i = 0;
            foreach (KeyValuePair<object, object> p in _paires) entrees[i++] = new DictionaryEntry(p.Key, p.Value);
            return new Enumerateur(entrees);
        }

        IEnumerator IEnumerable.GetEnumerator() { return GetEnumerator(); }

        // Instantané des paires : Current rend une DictionaryEntry encadrée.
        private sealed class Enumerateur : IDictionaryEnumerator
        {
            private readonly DictionaryEntry[] _entrees;
            private int _i = -1;

            public Enumerateur(DictionaryEntry[] entrees) { _entrees = entrees; }

            public bool MoveNext()
            {
                if (_i < _entrees.Length) _i++;
                return _i < _entrees.Length;
            }

            public void Reset() { _i = -1; }

            public DictionaryEntry Entry
            {
                get
                {
                    if (_i < 0 || _i >= _entrees.Length)
                        throw new InvalidOperationException("Enumeration has either not started or has already finished.");
                    return _entrees[_i];
                }
            }

            public object Key { get { return Entry.Key; } }
            public object Value { get { return Entry.Value; } }
            public object Current { get { return Entry; } }
        }

        // Keys et Values : copie des éléments au moment de l'appel.
        private sealed class CollectionFigee : ICollection
        {
            private readonly object[] _elements;

            public CollectionFigee(object[] elements) { _elements = elements; }
            public int Count { get { return _elements.Length; } }
            public IEnumerator GetEnumerator() { return new Parcours(_elements); }
        }

        private sealed class Parcours : IEnumerator
        {
            private readonly object[] _elements;
            private int _i = -1;

            public Parcours(object[] elements) { _elements = elements; }

            public bool MoveNext()
            {
                if (_i < _elements.Length) _i++;
                return _i < _elements.Length;
            }

            public void Reset() { _i = -1; }

            public object Current
            {
                get
                {
                    if (_i < 0 || _i >= _elements.Length)
                        throw new InvalidOperationException("Enumeration has either not started or has already finished.");
                    return _elements[_i];
                }
            }
        }
    }
}
