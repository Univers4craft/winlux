// System.StringComparer et l'interface non générique
// System.Collections.IEqualityComparer de la mscorlib Winlux (#2774, suivi
// #2752). Licence GPL 3. Écrit d'après la documentation publique de .NET
// Framework 4.x (MS Learn : « StringComparer Class », « StringComparer.Ordinal »,
// « StringComparer.Create », « IEqualityComparer Interface ») ; aucun code de
// Microsoft, de Mono ou de Wine.
//
// Les comparaisons reprennent celles de String.Compare et String.Equals
// (Chaines.cs) : culture actuelle et invariante comparent comme elles,
// l'ordinal compare les unités UTF-16.
using System.Collections;
using System.Collections.Generic;
using System.Globalization;

namespace System.Collections
{
    public interface IEqualityComparer
    {
        bool Equals(object x, object y);
        int GetHashCode(object obj);
    }
}

namespace System
{
    public abstract class StringComparer : IComparer, IEqualityComparer, IComparer<string>, IEqualityComparer<string>
    {
        private static readonly StringComparer s_ordinal = new ComparateurChaine(StringComparison.Ordinal);
        private static readonly StringComparer s_ordinalCasse = new ComparateurChaine(StringComparison.OrdinalIgnoreCase);
        private static readonly StringComparer s_invariant = new ComparateurChaine(StringComparison.InvariantCulture);
        private static readonly StringComparer s_invariantCasse = new ComparateurChaine(StringComparison.InvariantCultureIgnoreCase);

        protected StringComparer() { }

        public static StringComparer Ordinal { get { return s_ordinal; } }
        public static StringComparer OrdinalIgnoreCase { get { return s_ordinalCasse; } }
        public static StringComparer InvariantCulture { get { return s_invariant; } }
        public static StringComparer InvariantCultureIgnoreCase { get { return s_invariantCasse; } }
        public static StringComparer CurrentCulture { get { return new ComparateurChaine(StringComparison.CurrentCulture); } }
        public static StringComparer CurrentCultureIgnoreCase { get { return new ComparateurChaine(StringComparison.CurrentCultureIgnoreCase); } }

        public static StringComparer Create(CultureInfo culture, bool ignoreCase)
        {
            if (culture == null) throw new ArgumentNullException("culture");
            return new ComparateurChaine(ignoreCase ? StringComparison.CurrentCultureIgnoreCase : StringComparison.CurrentCulture);
        }

        public abstract int Compare(string x, string y);
        public abstract bool Equals(string x, string y);
        public abstract int GetHashCode(string obj);

        // Learn : deux chaînes passent par le comparateur de chaînes, sinon
        // IComparable ; ArgumentException si aucun des deux n'en est un.
        public int Compare(object x, object y)
        {
            if (x == y) return 0;
            if (x == null) return -1;
            if (y == null) return 1;
            string sx = x as string, sy = y as string;
            if (sx != null && sy != null) return Compare(sx, sy);
            IComparable c = x as IComparable;
            if (c != null) return c.CompareTo(y);
            throw new ArgumentException("At least one object must implement IComparable.");
        }

        public bool Equals(object x, object y)
        {
            if (x == y) return true;
            if (x == null || y == null) return false;
            string sx = x as string, sy = y as string;
            if (sx != null && sy != null) return Equals(sx, sy);
            return x.Equals(y);
        }

        public int GetHashCode(object obj)
        {
            if (obj == null) throw new ArgumentNullException("obj");
            string s = obj as string;
            return s != null ? GetHashCode(s) : obj.GetHashCode();
        }
    }

    internal sealed class ComparateurChaine : StringComparer
    {
        private readonly StringComparison _mode;

        internal ComparateurChaine(StringComparison mode) { _mode = mode; }

        private bool SansCasse
        {
            get
            {
                return _mode == StringComparison.OrdinalIgnoreCase || _mode == StringComparison.CurrentCultureIgnoreCase
                    || _mode == StringComparison.InvariantCultureIgnoreCase;
            }
        }

        public override int Compare(string x, string y) { return string.Compare(x, y, _mode); }
        public override bool Equals(string x, string y) { return string.Equals(x, y, _mode); }

        public override int GetHashCode(string obj)
        {
            if ((object)obj == null) throw new ArgumentNullException("obj");
            return SansCasse ? obj.ToUpperInvariant().GetHashCode() : obj.GetHashCode();
        }

        public override bool Equals(object obj)
        {
            ComparateurChaine autre = obj as ComparateurChaine;
            return autre != null && autre._mode == _mode;
        }

        public override int GetHashCode() { return (int)_mode; }
    }
}
