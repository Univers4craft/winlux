// LINQ to Objects (System.Linq.Enumerable), délégués Func/Action jusqu'à 16
// paramètres et Tuple dans la mscorlib Winlux (#1975).
//
// Écrit d'après la documentation publique de Microsoft Learn (« Enumerable
// Class » et la page de chaque opérateur : exécution différée ou immédiate,
// exceptions levées, ordre stable des tris) et ECMA-335 ; aucun code de
// Microsoft, de Mono ou de Wine.
using System.Collections;
using System.Collections.Generic;

namespace System
{
    public delegate void Action<in T1, in T2, in T3>(T1 arg1, T2 arg2, T3 arg3);
    public delegate void Action<in T1, in T2, in T3, in T4>(T1 arg1, T2 arg2, T3 arg3, T4 arg4);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, in T13>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, in T13, in T14>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, in T13, in T14, in T15>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15);
    public delegate void Action<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, in T13, in T14, in T15, in T16>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16);

    public delegate TResult Func<in T1, in T2, in T3, out TResult>(T1 arg1, T2 arg2, T3 arg3);
    public delegate TResult Func<in T1, in T2, in T3, in T4, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, in T13, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, in T13, in T14, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, in T13, in T14, in T15, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15);
    public delegate TResult Func<in T1, in T2, in T3, in T4, in T5, in T6, in T7, in T8, in T9, in T10, in T11, in T12, in T13, in T14, in T15, in T16, out TResult>(T1 arg1, T2 arg2, T3 arg3, T4 arg4, T5 arg5, T6 arg6, T7 arg7, T8 arg8, T9 arg9, T10 arg10, T11 arg11, T12 arg12, T13 arg13, T14 arg14, T15 arg15, T16 arg16);

    public static class Tuple
    {
        public static Tuple<T1> Create<T1>(T1 item1) { return new Tuple<T1>(item1); }
        public static Tuple<T1, T2> Create<T1, T2>(T1 item1, T2 item2) { return new Tuple<T1, T2>(item1, item2); }
        public static Tuple<T1, T2, T3> Create<T1, T2, T3>(T1 item1, T2 item2, T3 item3) { return new Tuple<T1, T2, T3>(item1, item2, item3); }
        public static Tuple<T1, T2, T3, T4> Create<T1, T2, T3, T4>(T1 item1, T2 item2, T3 item3, T4 item4) { return new Tuple<T1, T2, T3, T4>(item1, item2, item3, item4); }

        internal static bool Eq<T>(T a, T b) { return EqualityComparer<T>.Default.Equals(a, b); }
        internal static int Hash<T>(T a) { return a == null ? 0 : EqualityComparer<T>.Default.GetHashCode(a); }
        internal static int Combine(int h1, int h2) { return ((h1 << 5) + h1) ^ h2; }
        internal static string Text(object o) { return o == null ? "" : o.ToString(); }
    }

    public class Tuple<T1>
    {
        private readonly T1 _1;
        public Tuple(T1 item1) { _1 = item1; }
        public T1 Item1 { get { return _1; } }
        public override bool Equals(object obj)
        {
            Tuple<T1> o = obj as Tuple<T1>;
            return o != null && Tuple.Eq(_1, o._1);
        }
        public override int GetHashCode() { return Tuple.Hash(_1); }
        public override string ToString() { return "(" + Tuple.Text(_1) + ")"; }
    }

    public class Tuple<T1, T2>
    {
        private readonly T1 _1;
        private readonly T2 _2;
        public Tuple(T1 item1, T2 item2) { _1 = item1; _2 = item2; }
        public T1 Item1 { get { return _1; } }
        public T2 Item2 { get { return _2; } }
        public override bool Equals(object obj)
        {
            Tuple<T1, T2> o = obj as Tuple<T1, T2>;
            return o != null && Tuple.Eq(_1, o._1) && Tuple.Eq(_2, o._2);
        }
        public override int GetHashCode() { return Tuple.Combine(Tuple.Hash(_1), Tuple.Hash(_2)); }
        public override string ToString() { return "(" + Tuple.Text(_1) + ", " + Tuple.Text(_2) + ")"; }
    }

    public class Tuple<T1, T2, T3>
    {
        private readonly T1 _1;
        private readonly T2 _2;
        private readonly T3 _3;
        public Tuple(T1 item1, T2 item2, T3 item3) { _1 = item1; _2 = item2; _3 = item3; }
        public T1 Item1 { get { return _1; } }
        public T2 Item2 { get { return _2; } }
        public T3 Item3 { get { return _3; } }
        public override bool Equals(object obj)
        {
            Tuple<T1, T2, T3> o = obj as Tuple<T1, T2, T3>;
            return o != null && Tuple.Eq(_1, o._1) && Tuple.Eq(_2, o._2) && Tuple.Eq(_3, o._3);
        }
        public override int GetHashCode() { return Tuple.Combine(Tuple.Combine(Tuple.Hash(_1), Tuple.Hash(_2)), Tuple.Hash(_3)); }
        public override string ToString() { return "(" + Tuple.Text(_1) + ", " + Tuple.Text(_2) + ", " + Tuple.Text(_3) + ")"; }
    }

    public class Tuple<T1, T2, T3, T4>
    {
        private readonly T1 _1;
        private readonly T2 _2;
        private readonly T3 _3;
        private readonly T4 _4;
        public Tuple(T1 item1, T2 item2, T3 item3, T4 item4) { _1 = item1; _2 = item2; _3 = item3; _4 = item4; }
        public T1 Item1 { get { return _1; } }
        public T2 Item2 { get { return _2; } }
        public T3 Item3 { get { return _3; } }
        public T4 Item4 { get { return _4; } }
        public override bool Equals(object obj)
        {
            Tuple<T1, T2, T3, T4> o = obj as Tuple<T1, T2, T3, T4>;
            return o != null && Tuple.Eq(_1, o._1) && Tuple.Eq(_2, o._2) && Tuple.Eq(_3, o._3) && Tuple.Eq(_4, o._4);
        }
        public override int GetHashCode()
        {
            return Tuple.Combine(Tuple.Combine(Tuple.Hash(_1), Tuple.Hash(_2)), Tuple.Combine(Tuple.Hash(_3), Tuple.Hash(_4)));
        }
        public override string ToString()
        {
            return "(" + Tuple.Text(_1) + ", " + Tuple.Text(_2) + ", " + Tuple.Text(_3) + ", " + Tuple.Text(_4) + ")";
        }
    }
}

namespace System.Linq
{
    public interface IGrouping<out TKey, out TElement> : IEnumerable<TElement>
    {
        TKey Key { get; }
    }

    public interface ILookup<TKey, TElement> : IEnumerable<IGrouping<TKey, TElement>>
    {
        int Count { get; }
        IEnumerable<TElement> this[TKey key] { get; }
        bool Contains(TKey key);
    }

    public interface IOrderedEnumerable<TElement> : IEnumerable<TElement>
    {
        IOrderedEnumerable<TElement> CreateOrderedEnumerable<TKey>(Func<TElement, TKey> keySelector, IComparer<TKey> comparer, bool descending);
    }

    internal sealed class Grouping<TKey, TElement> : IGrouping<TKey, TElement>, IList<TElement>
    {
        private readonly TKey _key;
        internal readonly List<TElement> _items = new List<TElement>();

        internal Grouping(TKey key) { _key = key; }

        public TKey Key { get { return _key; } }
        public IEnumerator<TElement> GetEnumerator() { return _items.GetEnumerator(); }
        IEnumerator IEnumerable.GetEnumerator() { return _items.GetEnumerator(); }
        public int Count { get { return _items.Count; } }
        public bool IsReadOnly { get { return true; } }
        public TElement this[int index] { get { return _items[index]; } set { throw new NotSupportedException(); } }
        public bool Contains(TElement item) { return _items.Contains(item); }
        public void CopyTo(TElement[] array, int arrayIndex) { _items.CopyTo(array, arrayIndex); }
        public int IndexOf(TElement item) { return _items.IndexOf(item); }
        void ICollection<TElement>.Add(TElement item) { throw new NotSupportedException(); }
        void ICollection<TElement>.Clear() { throw new NotSupportedException(); }
        bool ICollection<TElement>.Remove(TElement item) { throw new NotSupportedException(); }
        void IList<TElement>.Insert(int index, TElement item) { throw new NotSupportedException(); }
        void IList<TElement>.RemoveAt(int index) { throw new NotSupportedException(); }
    }

    // Groupes dans l'ordre de première apparition de leur clé ; la clé
    // nulle a son propre groupe (Dictionary refuse null).
    public class Lookup<TKey, TElement> : ILookup<TKey, TElement>
    {
        private readonly Dictionary<TKey, Grouping<TKey, TElement>> _map;
        private readonly List<Grouping<TKey, TElement>> _order = new List<Grouping<TKey, TElement>>();
        private Grouping<TKey, TElement> _null;

        internal Lookup(IEqualityComparer<TKey> comparer)
        {
            _map = new Dictionary<TKey, Grouping<TKey, TElement>>(comparer);
        }

        internal Grouping<TKey, TElement> GetGroup(TKey key, bool create)
        {
            Grouping<TKey, TElement> g;
            if (key == null)
            {
                if (_null == null && create)
                {
                    _null = new Grouping<TKey, TElement>(key);
                    _order.Add(_null);
                }
                return _null;
            }
            if (_map.TryGetValue(key, out g)) return g;
            if (!create) return null;
            g = new Grouping<TKey, TElement>(key);
            _map.Add(key, g);
            _order.Add(g);
            return g;
        }

        public int Count { get { return _order.Count; } }

        public IEnumerable<TElement> this[TKey key]
        {
            get
            {
                Grouping<TKey, TElement> g = GetGroup(key, false);
                if (g == null) return new TElement[0];
                return g;
            }
        }

        public bool Contains(TKey key) { return GetGroup(key, false) != null; }

        public IEnumerator<IGrouping<TKey, TElement>> GetEnumerator()
        {
            foreach (Grouping<TKey, TElement> g in _order) yield return g;
        }

        IEnumerator IEnumerable.GetEnumerator() { return GetEnumerator(); }

        public IEnumerable<TResult> ApplyResultSelector<TResult>(Func<TKey, IEnumerable<TElement>, TResult> resultSelector)
        {
            foreach (Grouping<TKey, TElement> g in _order) yield return resultSelector(g.Key, g);
        }
    }

    // Tri stable (fusion) sur une ou plusieurs clés.
    internal abstract class Ordered<TElement> : IOrderedEnumerable<TElement>
    {
        internal IEnumerable<TElement> _source;
        internal Ordered<TElement> _parent;

        internal abstract int CompareKeys(TElement[] items, int[] keysIndex, int a, int b);
        internal abstract void ComputeKeys(TElement[] items);

        public IOrderedEnumerable<TElement> CreateOrderedEnumerable<TKey>(Func<TElement, TKey> keySelector, IComparer<TKey> comparer, bool descending)
        {
            OrderedBy<TElement, TKey> o = new OrderedBy<TElement, TKey>(_source, keySelector, comparer, descending);
            o._parent = this;
            return o;
        }

        private int Compare(TElement[] items, int a, int b)
        {
            // Clé principale d'abord : la chaîne va de la plus ancienne
            // (OrderBy) à la plus récente (ThenBy).
            List<Ordered<TElement>> chaine = new List<Ordered<TElement>>();
            for (Ordered<TElement> o = this; o != null; o = o._parent) chaine.Insert(0, o);
            foreach (Ordered<TElement> o in chaine)
            {
                int c = o.CompareKeys(items, null, a, b);
                if (c != 0) return c;
            }
            return a - b;
        }

        public IEnumerator<TElement> GetEnumerator()
        {
            List<TElement> l = new List<TElement>(_source);
            TElement[] items = l.ToArray();
            for (Ordered<TElement> o = this; o != null; o = o._parent) o.ComputeKeys(items);
            int[] idx = new int[items.Length];
            for (int i = 0; i < idx.Length; i++) idx[i] = i;
            int[] tmp = new int[idx.Length];
            MergeSort(items, idx, tmp, 0, idx.Length);
            for (int i = 0; i < idx.Length; i++) yield return items[idx[i]];
        }

        private void MergeSort(TElement[] items, int[] idx, int[] tmp, int lo, int hi)
        {
            if (hi - lo < 2) return;
            int mid = (lo + hi) / 2;
            MergeSort(items, idx, tmp, lo, mid);
            MergeSort(items, idx, tmp, mid, hi);
            int i = lo, j = mid, k = lo;
            while (i < mid && j < hi) tmp[k++] = Compare(items, idx[i], idx[j]) <= 0 ? idx[i++] : idx[j++];
            while (i < mid) tmp[k++] = idx[i++];
            while (j < hi) tmp[k++] = idx[j++];
            for (k = lo; k < hi; k++) idx[k] = tmp[k];
        }

        IEnumerator IEnumerable.GetEnumerator() { return GetEnumerator(); }
    }

    internal sealed class OrderedBy<TElement, TKey> : Ordered<TElement>
    {
        private readonly Func<TElement, TKey> _keySelector;
        private readonly IComparer<TKey> _comparer;
        private readonly bool _descending;
        private TKey[] _keys;

        internal OrderedBy(IEnumerable<TElement> source, Func<TElement, TKey> keySelector, IComparer<TKey> comparer, bool descending)
        {
            if (source == null) throw new ArgumentNullException("source");
            if (keySelector == null) throw new ArgumentNullException("keySelector");
            _source = source;
            _keySelector = keySelector;
            _comparer = comparer ?? Comparer<TKey>.Default;
            _descending = descending;
        }

        internal override void ComputeKeys(TElement[] items)
        {
            _keys = new TKey[items.Length];
            for (int i = 0; i < items.Length; i++) _keys[i] = _keySelector(items[i]);
        }

        internal override int CompareKeys(TElement[] items, int[] keysIndex, int a, int b)
        {
            int c = _comparer.Compare(_keys[a], _keys[b]);
            return _descending ? -c : c;
        }
    }

    public static class Enumerable
    {
        private static void Check(object o, string name)
        {
            if (o == null) throw new ArgumentNullException(name);
        }

        private static InvalidOperationException NoElements() { return new InvalidOperationException("Sequence contains no elements"); }
        private static InvalidOperationException NoMatch() { return new InvalidOperationException("Sequence contains no matching element"); }
        private static InvalidOperationException MoreThanOne() { return new InvalidOperationException("Sequence contains more than one element"); }
        private static InvalidOperationException MoreThanOneMatch() { return new InvalidOperationException("Sequence contains more than one matching element"); }

        // --- Filtrage et projection (différés) ---

        public static IEnumerable<TSource> Where<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            return WhereIt(source, predicate);
        }

        private static IEnumerable<TSource> WhereIt<TSource>(IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            foreach (TSource x in source) if (predicate(x)) yield return x;
        }

        public static IEnumerable<TSource> Where<TSource>(this IEnumerable<TSource> source, Func<TSource, int, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            return WhereIndexIt(source, predicate);
        }

        private static IEnumerable<TSource> WhereIndexIt<TSource>(IEnumerable<TSource> source, Func<TSource, int, bool> predicate)
        {
            int i = 0;
            foreach (TSource x in source) if (predicate(x, i++)) yield return x;
        }

        public static IEnumerable<TResult> Select<TSource, TResult>(this IEnumerable<TSource> source, Func<TSource, TResult> selector)
        {
            Check(source, "source");
            Check(selector, "selector");
            return SelectIt(source, selector);
        }

        private static IEnumerable<TResult> SelectIt<TSource, TResult>(IEnumerable<TSource> source, Func<TSource, TResult> selector)
        {
            foreach (TSource x in source) yield return selector(x);
        }

        public static IEnumerable<TResult> Select<TSource, TResult>(this IEnumerable<TSource> source, Func<TSource, int, TResult> selector)
        {
            Check(source, "source");
            Check(selector, "selector");
            return SelectIndexIt(source, selector);
        }

        private static IEnumerable<TResult> SelectIndexIt<TSource, TResult>(IEnumerable<TSource> source, Func<TSource, int, TResult> selector)
        {
            int i = 0;
            foreach (TSource x in source) yield return selector(x, i++);
        }

        public static IEnumerable<TResult> SelectMany<TSource, TResult>(this IEnumerable<TSource> source, Func<TSource, IEnumerable<TResult>> selector)
        {
            Check(source, "source");
            Check(selector, "selector");
            return SelectManyIt(source, selector);
        }

        private static IEnumerable<TResult> SelectManyIt<TSource, TResult>(IEnumerable<TSource> source, Func<TSource, IEnumerable<TResult>> selector)
        {
            foreach (TSource x in source)
                foreach (TResult y in selector(x)) yield return y;
        }

        public static IEnumerable<TResult> SelectMany<TSource, TResult>(this IEnumerable<TSource> source, Func<TSource, int, IEnumerable<TResult>> selector)
        {
            Check(source, "source");
            Check(selector, "selector");
            return SelectManyIndexIt(source, selector);
        }

        private static IEnumerable<TResult> SelectManyIndexIt<TSource, TResult>(IEnumerable<TSource> source, Func<TSource, int, IEnumerable<TResult>> selector)
        {
            int i = 0;
            foreach (TSource x in source)
                foreach (TResult y in selector(x, i++)) yield return y;
        }

        public static IEnumerable<TResult> SelectMany<TSource, TCollection, TResult>(this IEnumerable<TSource> source, Func<TSource, IEnumerable<TCollection>> collectionSelector, Func<TSource, TCollection, TResult> resultSelector)
        {
            Check(source, "source");
            Check(collectionSelector, "collectionSelector");
            Check(resultSelector, "resultSelector");
            return SelectManyResultIt(source, collectionSelector, resultSelector);
        }

        private static IEnumerable<TResult> SelectManyResultIt<TSource, TCollection, TResult>(IEnumerable<TSource> source, Func<TSource, IEnumerable<TCollection>> collectionSelector, Func<TSource, TCollection, TResult> resultSelector)
        {
            foreach (TSource x in source)
                foreach (TCollection y in collectionSelector(x)) yield return resultSelector(x, y);
        }

        public static IEnumerable<TResult> Cast<TResult>(this IEnumerable source)
        {
            Check(source, "source");
            IEnumerable<TResult> deja = source as IEnumerable<TResult>;
            if (deja != null) return deja;
            return CastIt<TResult>(source);
        }

        private static IEnumerable<TResult> CastIt<TResult>(IEnumerable source)
        {
            foreach (object o in source) yield return (TResult)o;
        }

        public static IEnumerable<TResult> OfType<TResult>(this IEnumerable source)
        {
            Check(source, "source");
            return OfTypeIt<TResult>(source);
        }

        private static IEnumerable<TResult> OfTypeIt<TResult>(IEnumerable source)
        {
            foreach (object o in source) if (o is TResult) yield return (TResult)o;
        }

        public static IEnumerable<TSource> AsEnumerable<TSource>(this IEnumerable<TSource> source) { return source; }

        // --- Partition (différée) ---

        public static IEnumerable<TSource> Skip<TSource>(this IEnumerable<TSource> source, int count)
        {
            Check(source, "source");
            return SkipIt(source, count);
        }

        private static IEnumerable<TSource> SkipIt<TSource>(IEnumerable<TSource> source, int count)
        {
            int i = 0;
            foreach (TSource x in source) if (i++ >= count) yield return x;
        }

        public static IEnumerable<TSource> Take<TSource>(this IEnumerable<TSource> source, int count)
        {
            Check(source, "source");
            return TakeIt(source, count);
        }

        private static IEnumerable<TSource> TakeIt<TSource>(IEnumerable<TSource> source, int count)
        {
            if (count <= 0) yield break;
            int i = 0;
            foreach (TSource x in source)
            {
                yield return x;
                if (++i >= count) yield break;
            }
        }

        public static IEnumerable<TSource> SkipWhile<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            return SkipWhileIt(source, predicate);
        }

        private static IEnumerable<TSource> SkipWhileIt<TSource>(IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            bool passe = false;
            foreach (TSource x in source)
            {
                if (!passe && predicate(x)) continue;
                passe = true;
                yield return x;
            }
        }

        public static IEnumerable<TSource> TakeWhile<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            return TakeWhileIt(source, predicate);
        }

        private static IEnumerable<TSource> TakeWhileIt<TSource>(IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            foreach (TSource x in source)
            {
                if (!predicate(x)) yield break;
                yield return x;
            }
        }

        // --- Ensembles (différés) ---

        public static IEnumerable<TSource> Concat<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second)
        {
            Check(first, "first");
            Check(second, "second");
            return ConcatIt(first, second);
        }

        private static IEnumerable<TSource> ConcatIt<TSource>(IEnumerable<TSource> first, IEnumerable<TSource> second)
        {
            foreach (TSource x in first) yield return x;
            foreach (TSource x in second) yield return x;
        }

        public static IEnumerable<TSource> Distinct<TSource>(this IEnumerable<TSource> source) { return Distinct(source, null); }

        public static IEnumerable<TSource> Distinct<TSource>(this IEnumerable<TSource> source, IEqualityComparer<TSource> comparer)
        {
            Check(source, "source");
            return DistinctIt(source, comparer);
        }

        private static IEnumerable<TSource> DistinctIt<TSource>(IEnumerable<TSource> source, IEqualityComparer<TSource> comparer)
        {
            Ensemble<TSource> vus = new Ensemble<TSource>(comparer);
            foreach (TSource x in source) if (vus.Add(x)) yield return x;
        }

        public static IEnumerable<TSource> Union<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second) { return Union(first, second, null); }

        public static IEnumerable<TSource> Union<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second, IEqualityComparer<TSource> comparer)
        {
            Check(first, "first");
            Check(second, "second");
            return UnionIt(first, second, comparer);
        }

        private static IEnumerable<TSource> UnionIt<TSource>(IEnumerable<TSource> first, IEnumerable<TSource> second, IEqualityComparer<TSource> comparer)
        {
            Ensemble<TSource> vus = new Ensemble<TSource>(comparer);
            foreach (TSource x in first) if (vus.Add(x)) yield return x;
            foreach (TSource x in second) if (vus.Add(x)) yield return x;
        }

        public static IEnumerable<TSource> Intersect<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second) { return Intersect(first, second, null); }

        public static IEnumerable<TSource> Intersect<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second, IEqualityComparer<TSource> comparer)
        {
            Check(first, "first");
            Check(second, "second");
            return IntersectIt(first, second, comparer);
        }

        private static IEnumerable<TSource> IntersectIt<TSource>(IEnumerable<TSource> first, IEnumerable<TSource> second, IEqualityComparer<TSource> comparer)
        {
            Ensemble<TSource> b = new Ensemble<TSource>(comparer);
            foreach (TSource x in second) b.Add(x);
            foreach (TSource x in first) if (b.Remove(x)) yield return x;
        }

        public static IEnumerable<TSource> Except<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second) { return Except(first, second, null); }

        public static IEnumerable<TSource> Except<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second, IEqualityComparer<TSource> comparer)
        {
            Check(first, "first");
            Check(second, "second");
            return ExceptIt(first, second, comparer);
        }

        private static IEnumerable<TSource> ExceptIt<TSource>(IEnumerable<TSource> first, IEnumerable<TSource> second, IEqualityComparer<TSource> comparer)
        {
            Ensemble<TSource> vus = new Ensemble<TSource>(comparer);
            foreach (TSource x in second) vus.Add(x);
            foreach (TSource x in first) if (vus.Add(x)) yield return x;
        }

        // Ensemble qui accepte null (HashSet n'y est pas tenu ici).
        private sealed class Ensemble<T>
        {
            private readonly HashSet<T> _set;
            private bool _null;

            public Ensemble(IEqualityComparer<T> comparer) { _set = new HashSet<T>(comparer); }

            public bool Add(T x)
            {
                if (x == null)
                {
                    if (_null) return false;
                    _null = true;
                    return true;
                }
                return _set.Add(x);
            }

            public bool Remove(T x)
            {
                if (x == null)
                {
                    bool etait = _null;
                    _null = false;
                    return etait;
                }
                return _set.Remove(x);
            }
        }

        public static IEnumerable<TSource> Reverse<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            return ReverseIt(source);
        }

        private static IEnumerable<TSource> ReverseIt<TSource>(IEnumerable<TSource> source)
        {
            TSource[] a = ToArray(source);
            for (int i = a.Length - 1; i >= 0; i--) yield return a[i];
        }

        public static IEnumerable<TSource> DefaultIfEmpty<TSource>(this IEnumerable<TSource> source) { return DefaultIfEmpty(source, default(TSource)); }

        public static IEnumerable<TSource> DefaultIfEmpty<TSource>(this IEnumerable<TSource> source, TSource defaultValue)
        {
            Check(source, "source");
            return DefaultIfEmptyIt(source, defaultValue);
        }

        private static IEnumerable<TSource> DefaultIfEmptyIt<TSource>(IEnumerable<TSource> source, TSource defaultValue)
        {
            bool vide = true;
            foreach (TSource x in source)
            {
                vide = false;
                yield return x;
            }
            if (vide) yield return defaultValue;
        }

        public static IEnumerable<TResult> Zip<TFirst, TSecond, TResult>(this IEnumerable<TFirst> first, IEnumerable<TSecond> second, Func<TFirst, TSecond, TResult> resultSelector)
        {
            Check(first, "first");
            Check(second, "second");
            Check(resultSelector, "resultSelector");
            return ZipIt(first, second, resultSelector);
        }

        private static IEnumerable<TResult> ZipIt<TFirst, TSecond, TResult>(IEnumerable<TFirst> first, IEnumerable<TSecond> second, Func<TFirst, TSecond, TResult> resultSelector)
        {
            using (IEnumerator<TFirst> a = first.GetEnumerator())
            using (IEnumerator<TSecond> b = second.GetEnumerator())
            {
                while (a.MoveNext() && b.MoveNext()) yield return resultSelector(a.Current, b.Current);
            }
        }

        // --- Génération ---

        public static IEnumerable<TResult> Empty<TResult>() { return new TResult[0]; }

        public static IEnumerable<int> Range(int start, int count)
        {
            if (count < 0 || (long)start + count - 1 > int.MaxValue) throw new ArgumentOutOfRangeException("count");
            return RangeIt(start, count);
        }

        private static IEnumerable<int> RangeIt(int start, int count)
        {
            for (int i = 0; i < count; i++) yield return start + i;
        }

        public static IEnumerable<TResult> Repeat<TResult>(TResult element, int count)
        {
            if (count < 0) throw new ArgumentOutOfRangeException("count");
            return RepeatIt(element, count);
        }

        private static IEnumerable<TResult> RepeatIt<TResult>(TResult element, int count)
        {
            for (int i = 0; i < count; i++) yield return element;
        }

        // --- Tri et regroupement ---

        public static IOrderedEnumerable<TSource> OrderBy<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector)
        {
            return new OrderedBy<TSource, TKey>(source, keySelector, null, false);
        }

        public static IOrderedEnumerable<TSource> OrderBy<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, IComparer<TKey> comparer)
        {
            return new OrderedBy<TSource, TKey>(source, keySelector, comparer, false);
        }

        public static IOrderedEnumerable<TSource> OrderByDescending<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector)
        {
            return new OrderedBy<TSource, TKey>(source, keySelector, null, true);
        }

        public static IOrderedEnumerable<TSource> OrderByDescending<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, IComparer<TKey> comparer)
        {
            return new OrderedBy<TSource, TKey>(source, keySelector, comparer, true);
        }

        public static IOrderedEnumerable<TSource> ThenBy<TSource, TKey>(this IOrderedEnumerable<TSource> source, Func<TSource, TKey> keySelector)
        {
            Check(source, "source");
            return source.CreateOrderedEnumerable(keySelector, null, false);
        }

        public static IOrderedEnumerable<TSource> ThenBy<TSource, TKey>(this IOrderedEnumerable<TSource> source, Func<TSource, TKey> keySelector, IComparer<TKey> comparer)
        {
            Check(source, "source");
            return source.CreateOrderedEnumerable(keySelector, comparer, false);
        }

        public static IOrderedEnumerable<TSource> ThenByDescending<TSource, TKey>(this IOrderedEnumerable<TSource> source, Func<TSource, TKey> keySelector)
        {
            Check(source, "source");
            return source.CreateOrderedEnumerable(keySelector, null, true);
        }

        public static IOrderedEnumerable<TSource> ThenByDescending<TSource, TKey>(this IOrderedEnumerable<TSource> source, Func<TSource, TKey> keySelector, IComparer<TKey> comparer)
        {
            Check(source, "source");
            return source.CreateOrderedEnumerable(keySelector, comparer, true);
        }

        public static IEnumerable<IGrouping<TKey, TSource>> GroupBy<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector)
        {
            return GroupBy(source, keySelector, null);
        }

        public static IEnumerable<IGrouping<TKey, TSource>> GroupBy<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, IEqualityComparer<TKey> comparer)
        {
            Check(source, "source");
            Check(keySelector, "keySelector");
            return GroupByIt(source, keySelector, comparer);
        }

        private static IEnumerable<IGrouping<TKey, TSource>> GroupByIt<TSource, TKey>(IEnumerable<TSource> source, Func<TSource, TKey> keySelector, IEqualityComparer<TKey> comparer)
        {
            foreach (IGrouping<TKey, TSource> g in ToLookup(source, keySelector, comparer)) yield return g;
        }

        public static IEnumerable<IGrouping<TKey, TElement>> GroupBy<TSource, TKey, TElement>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, Func<TSource, TElement> elementSelector)
        {
            Check(source, "source");
            Check(keySelector, "keySelector");
            Check(elementSelector, "elementSelector");
            return GroupByElementIt(source, keySelector, elementSelector);
        }

        private static IEnumerable<IGrouping<TKey, TElement>> GroupByElementIt<TSource, TKey, TElement>(IEnumerable<TSource> source, Func<TSource, TKey> keySelector, Func<TSource, TElement> elementSelector)
        {
            foreach (IGrouping<TKey, TElement> g in ToLookup(source, keySelector, elementSelector)) yield return g;
        }

        public static IEnumerable<TResult> GroupBy<TSource, TKey, TResult>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, Func<TKey, IEnumerable<TSource>, TResult> resultSelector)
        {
            Check(source, "source");
            Check(keySelector, "keySelector");
            Check(resultSelector, "resultSelector");
            return GroupByResultIt(source, keySelector, resultSelector);
        }

        private static IEnumerable<TResult> GroupByResultIt<TSource, TKey, TResult>(IEnumerable<TSource> source, Func<TSource, TKey> keySelector, Func<TKey, IEnumerable<TSource>, TResult> resultSelector)
        {
            foreach (TResult r in ((Lookup<TKey, TSource>)ToLookup(source, keySelector, (IEqualityComparer<TKey>)null)).ApplyResultSelector(resultSelector)) yield return r;
        }

        public static ILookup<TKey, TSource> ToLookup<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector)
        {
            return ToLookup(source, keySelector, (IEqualityComparer<TKey>)null);
        }

        public static ILookup<TKey, TSource> ToLookup<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, IEqualityComparer<TKey> comparer)
        {
            Check(source, "source");
            Check(keySelector, "keySelector");
            Lookup<TKey, TSource> l = new Lookup<TKey, TSource>(comparer);
            foreach (TSource x in source) l.GetGroup(keySelector(x), true)._items.Add(x);
            return l;
        }

        public static ILookup<TKey, TElement> ToLookup<TSource, TKey, TElement>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, Func<TSource, TElement> elementSelector)
        {
            Check(source, "source");
            Check(keySelector, "keySelector");
            Check(elementSelector, "elementSelector");
            Lookup<TKey, TElement> l = new Lookup<TKey, TElement>(null);
            foreach (TSource x in source) l.GetGroup(keySelector(x), true)._items.Add(elementSelector(x));
            return l;
        }

        public static IEnumerable<TResult> Join<TOuter, TInner, TKey, TResult>(this IEnumerable<TOuter> outer, IEnumerable<TInner> inner, Func<TOuter, TKey> outerKeySelector, Func<TInner, TKey> innerKeySelector, Func<TOuter, TInner, TResult> resultSelector)
        {
            Check(outer, "outer");
            Check(inner, "inner");
            Check(outerKeySelector, "outerKeySelector");
            Check(innerKeySelector, "innerKeySelector");
            Check(resultSelector, "resultSelector");
            return JoinIt(outer, inner, outerKeySelector, innerKeySelector, resultSelector);
        }

        private static IEnumerable<TResult> JoinIt<TOuter, TInner, TKey, TResult>(IEnumerable<TOuter> outer, IEnumerable<TInner> inner, Func<TOuter, TKey> outerKeySelector, Func<TInner, TKey> innerKeySelector, Func<TOuter, TInner, TResult> resultSelector)
        {
            Lookup<TKey, TInner> l = (Lookup<TKey, TInner>)ToLookup(inner, innerKeySelector, (IEqualityComparer<TKey>)null);
            foreach (TOuter o in outer)
            {
                TKey k = outerKeySelector(o);
                if (k == null) continue;
                Grouping<TKey, TInner> g = l.GetGroup(k, false);
                if (g == null) continue;
                foreach (TInner i in g._items) yield return resultSelector(o, i);
            }
        }

        // --- Conversion (immédiate) ---

        public static TSource[] ToArray<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            return new List<TSource>(source).ToArray();
        }

        public static List<TSource> ToList<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            return new List<TSource>(source);
        }

        public static Dictionary<TKey, TSource> ToDictionary<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector)
        {
            return ToDictionary(source, keySelector, (IEqualityComparer<TKey>)null);
        }

        public static Dictionary<TKey, TSource> ToDictionary<TSource, TKey>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, IEqualityComparer<TKey> comparer)
        {
            Check(source, "source");
            Check(keySelector, "keySelector");
            Dictionary<TKey, TSource> d = new Dictionary<TKey, TSource>(comparer);
            foreach (TSource x in source) d.Add(keySelector(x), x);
            return d;
        }

        public static Dictionary<TKey, TElement> ToDictionary<TSource, TKey, TElement>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, Func<TSource, TElement> elementSelector)
        {
            return ToDictionary(source, keySelector, elementSelector, null);
        }

        public static Dictionary<TKey, TElement> ToDictionary<TSource, TKey, TElement>(this IEnumerable<TSource> source, Func<TSource, TKey> keySelector, Func<TSource, TElement> elementSelector, IEqualityComparer<TKey> comparer)
        {
            Check(source, "source");
            Check(keySelector, "keySelector");
            Check(elementSelector, "elementSelector");
            Dictionary<TKey, TElement> d = new Dictionary<TKey, TElement>(comparer);
            foreach (TSource x in source) d.Add(keySelector(x), elementSelector(x));
            return d;
        }

        // --- Éléments (immédiats) ---

        public static TSource First<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            foreach (TSource x in source) return x;
            throw NoElements();
        }

        public static TSource First<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            foreach (TSource x in source) if (predicate(x)) return x;
            throw NoMatch();
        }

        public static TSource FirstOrDefault<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            foreach (TSource x in source) return x;
            return default(TSource);
        }

        public static TSource FirstOrDefault<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            foreach (TSource x in source) if (predicate(x)) return x;
            return default(TSource);
        }

        public static TSource Last<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            bool vu = false;
            TSource r = default(TSource);
            foreach (TSource x in source) { r = x; vu = true; }
            if (!vu) throw NoElements();
            return r;
        }

        public static TSource Last<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            bool vu = false;
            TSource r = default(TSource);
            foreach (TSource x in source) if (predicate(x)) { r = x; vu = true; }
            if (!vu) throw NoMatch();
            return r;
        }

        public static TSource LastOrDefault<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            TSource r = default(TSource);
            foreach (TSource x in source) r = x;
            return r;
        }

        public static TSource LastOrDefault<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            TSource r = default(TSource);
            foreach (TSource x in source) if (predicate(x)) r = x;
            return r;
        }

        public static TSource Single<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            int n = 0;
            TSource r = default(TSource);
            foreach (TSource x in source)
            {
                if (++n > 1) throw MoreThanOne();
                r = x;
            }
            if (n == 0) throw NoElements();
            return r;
        }

        public static TSource Single<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            int n = 0;
            TSource r = default(TSource);
            foreach (TSource x in source)
            {
                if (!predicate(x)) continue;
                if (++n > 1) throw MoreThanOneMatch();
                r = x;
            }
            if (n == 0) throw NoMatch();
            return r;
        }

        public static TSource SingleOrDefault<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            int n = 0;
            TSource r = default(TSource);
            foreach (TSource x in source)
            {
                if (++n > 1) throw MoreThanOne();
                r = x;
            }
            return r;
        }

        public static TSource SingleOrDefault<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            int n = 0;
            TSource r = default(TSource);
            foreach (TSource x in source)
            {
                if (!predicate(x)) continue;
                if (++n > 1) throw MoreThanOneMatch();
                r = x;
            }
            return r;
        }

        public static TSource ElementAt<TSource>(this IEnumerable<TSource> source, int index)
        {
            Check(source, "source");
            if (index >= 0)
            {
                IList<TSource> l = source as IList<TSource>;
                if (l != null)
                {
                    if (index < l.Count) return l[index];
                }
                else
                {
                    int i = 0;
                    foreach (TSource x in source) if (i++ == index) return x;
                }
            }
            throw new ArgumentOutOfRangeException("index");
        }

        public static TSource ElementAtOrDefault<TSource>(this IEnumerable<TSource> source, int index)
        {
            Check(source, "source");
            if (index < 0) return default(TSource);
            int i = 0;
            foreach (TSource x in source) if (i++ == index) return x;
            return default(TSource);
        }

        // --- Quantificateurs et agrégats (immédiats) ---

        public static bool Any<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            using (IEnumerator<TSource> e = source.GetEnumerator()) return e.MoveNext();
        }

        public static bool Any<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            foreach (TSource x in source) if (predicate(x)) return true;
            return false;
        }

        public static bool All<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            foreach (TSource x in source) if (!predicate(x)) return false;
            return true;
        }

        public static bool Contains<TSource>(this IEnumerable<TSource> source, TSource value)
        {
            Check(source, "source");
            ICollection<TSource> c = source as ICollection<TSource>;
            if (c != null) return c.Contains(value);
            return Contains(source, value, null);
        }

        public static bool Contains<TSource>(this IEnumerable<TSource> source, TSource value, IEqualityComparer<TSource> comparer)
        {
            Check(source, "source");
            if (comparer == null) comparer = EqualityComparer<TSource>.Default;
            foreach (TSource x in source) if (comparer.Equals(x, value)) return true;
            return false;
        }

        public static bool SequenceEqual<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second)
        {
            return SequenceEqual(first, second, null);
        }

        public static bool SequenceEqual<TSource>(this IEnumerable<TSource> first, IEnumerable<TSource> second, IEqualityComparer<TSource> comparer)
        {
            Check(first, "first");
            Check(second, "second");
            if (comparer == null) comparer = EqualityComparer<TSource>.Default;
            using (IEnumerator<TSource> a = first.GetEnumerator())
            using (IEnumerator<TSource> b = second.GetEnumerator())
            {
                while (a.MoveNext())
                {
                    if (!b.MoveNext() || !comparer.Equals(a.Current, b.Current)) return false;
                }
                return !b.MoveNext();
            }
        }

        public static int Count<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            ICollection<TSource> c = source as ICollection<TSource>;
            if (c != null) return c.Count;
            int n = 0;
            using (IEnumerator<TSource> e = source.GetEnumerator())
                while (e.MoveNext()) n++;
            return n;
        }

        public static int Count<TSource>(this IEnumerable<TSource> source, Func<TSource, bool> predicate)
        {
            Check(source, "source");
            Check(predicate, "predicate");
            int n = 0;
            foreach (TSource x in source) if (predicate(x)) n++;
            return n;
        }

        public static long LongCount<TSource>(this IEnumerable<TSource> source)
        {
            Check(source, "source");
            long n = 0;
            using (IEnumerator<TSource> e = source.GetEnumerator())
                while (e.MoveNext()) n++;
            return n;
        }

        public static TSource Aggregate<TSource>(this IEnumerable<TSource> source, Func<TSource, TSource, TSource> func)
        {
            Check(source, "source");
            Check(func, "func");
            using (IEnumerator<TSource> e = source.GetEnumerator())
            {
                if (!e.MoveNext()) throw NoElements();
                TSource r = e.Current;
                while (e.MoveNext()) r = func(r, e.Current);
                return r;
            }
        }

        public static TAccumulate Aggregate<TSource, TAccumulate>(this IEnumerable<TSource> source, TAccumulate seed, Func<TAccumulate, TSource, TAccumulate> func)
        {
            Check(source, "source");
            Check(func, "func");
            TAccumulate r = seed;
            foreach (TSource x in source) r = func(r, x);
            return r;
        }

        public static TResult Aggregate<TSource, TAccumulate, TResult>(this IEnumerable<TSource> source, TAccumulate seed, Func<TAccumulate, TSource, TAccumulate> func, Func<TAccumulate, TResult> resultSelector)
        {
            Check(resultSelector, "resultSelector");
            return resultSelector(Aggregate(source, seed, func));
        }

        public static int Sum(this IEnumerable<int> source)
        {
            Check(source, "source");
            int s = 0;
            foreach (int x in source) s = checked(s + x);
            return s;
        }

        public static long Sum(this IEnumerable<long> source)
        {
            Check(source, "source");
            long s = 0;
            foreach (long x in source) s = checked(s + x);
            return s;
        }

        public static double Sum(this IEnumerable<double> source)
        {
            Check(source, "source");
            double s = 0;
            foreach (double x in source) s += x;
            return s;
        }

        public static float Sum(this IEnumerable<float> source)
        {
            Check(source, "source");
            double s = 0;
            foreach (float x in source) s += x;
            return (float)s;
        }

        public static int Sum<TSource>(this IEnumerable<TSource> source, Func<TSource, int> selector) { return Sum(Select(source, selector)); }
        public static long Sum<TSource>(this IEnumerable<TSource> source, Func<TSource, long> selector) { return Sum(Select(source, selector)); }
        public static double Sum<TSource>(this IEnumerable<TSource> source, Func<TSource, double> selector) { return Sum(Select(source, selector)); }
        public static float Sum<TSource>(this IEnumerable<TSource> source, Func<TSource, float> selector) { return Sum(Select(source, selector)); }

        public static double Average(this IEnumerable<int> source)
        {
            Check(source, "source");
            long s = 0;
            long n = 0;
            foreach (int x in source) { s += x; n++; }
            if (n == 0) throw NoElements();
            return (double)s / n;
        }

        public static double Average(this IEnumerable<long> source)
        {
            Check(source, "source");
            double s = 0;
            long n = 0;
            foreach (long x in source) { s += x; n++; }
            if (n == 0) throw NoElements();
            return s / n;
        }

        public static double Average(this IEnumerable<double> source)
        {
            Check(source, "source");
            double s = 0;
            long n = 0;
            foreach (double x in source) { s += x; n++; }
            if (n == 0) throw NoElements();
            return s / n;
        }

        public static double Average<TSource>(this IEnumerable<TSource> source, Func<TSource, int> selector) { return Average(Select(source, selector)); }
        public static double Average<TSource>(this IEnumerable<TSource> source, Func<TSource, double> selector) { return Average(Select(source, selector)); }

        // Min et Max : le premier élément l'emporte en cas d'égalité ; une
        // séquence vide lève, sauf pour un type qui accepte null.
        private static TSource Extreme<TSource>(IEnumerable<TSource> source, int signe)
        {
            Check(source, "source");
            Comparer<TSource> cmp = Comparer<TSource>.Default;
            bool vu = false;
            TSource r = default(TSource);
            foreach (TSource x in source)
            {
                if (x == null) continue;
                if (!vu || cmp.Compare(x, r) * signe > 0)
                {
                    r = x;
                    vu = true;
                }
            }
            if (!vu && default(TSource) != null) throw NoElements();
            return r;
        }

        public static int Max(this IEnumerable<int> source) { return Extreme(source, 1); }
        public static long Max(this IEnumerable<long> source) { return Extreme(source, 1); }
        public static double Max(this IEnumerable<double> source) { return Extreme(source, 1); }
        public static TSource Max<TSource>(this IEnumerable<TSource> source) { return Extreme(source, 1); }
        public static int Max<TSource>(this IEnumerable<TSource> source, Func<TSource, int> selector) { return Extreme(Select(source, selector), 1); }
        public static long Max<TSource>(this IEnumerable<TSource> source, Func<TSource, long> selector) { return Extreme(Select(source, selector), 1); }
        public static double Max<TSource>(this IEnumerable<TSource> source, Func<TSource, double> selector) { return Extreme(Select(source, selector), 1); }
        public static TResult Max<TSource, TResult>(this IEnumerable<TSource> source, Func<TSource, TResult> selector) { return Extreme(Select(source, selector), 1); }

        public static int Min(this IEnumerable<int> source) { return Extreme(source, -1); }
        public static long Min(this IEnumerable<long> source) { return Extreme(source, -1); }
        public static double Min(this IEnumerable<double> source) { return Extreme(source, -1); }
        public static TSource Min<TSource>(this IEnumerable<TSource> source) { return Extreme(source, -1); }
        public static int Min<TSource>(this IEnumerable<TSource> source, Func<TSource, int> selector) { return Extreme(Select(source, selector), -1); }
        public static long Min<TSource>(this IEnumerable<TSource> source, Func<TSource, long> selector) { return Extreme(Select(source, selector), -1); }
        public static double Min<TSource>(this IEnumerable<TSource> source, Func<TSource, double> selector) { return Extreme(Select(source, selector), -1); }
        public static TResult Min<TSource, TResult>(this IEnumerable<TSource> source, Func<TSource, TResult> selector) { return Extreme(Select(source, selector), -1); }
    }
}
