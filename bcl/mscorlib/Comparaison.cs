// BCL Winlux : IComparable, IComparer, IComparer<T> et Comparer<T> (#1955).
// Licence GPL 3. Écrit d'après la documentation publique de l'API
// (« Comparer<T>.Default Property », « IComparable Interface ») ; aucun code
// de Microsoft, de Mono ou de Wine.
//
// Comparer<T>.Default : null avant toute valeur, puis IComparable<T>, puis
// IComparable. Les primitives et les chaînes sont comparées directement,
// sans passer par leurs interfaces.

using System.Collections;
using System.Collections.Generic;

namespace System
{
    public interface IComparable
    {
        int CompareTo(object obj);
    }
}

namespace System.Collections
{
    public interface IComparer
    {
        int Compare(object x, object y);
    }
}

namespace System.Collections.Generic
{
    public interface IComparer<in T>
    {
        int Compare(T x, T y);
    }

    public abstract class Comparer<T> : IComparer, IComparer<T>
    {
        private static Comparer<T> _default;

        public static Comparer<T> Default
        {
            get
            {
                if (_default == null)
                    _default = new ObjectComparer<T>();
                return _default;
            }
        }

        public static Comparer<T> Create(Comparison<T> comparison)
        {
            if (comparison == null)
                throw new ArgumentNullException("comparison");
            return new ComparisonComparer<T>(comparison);
        }

        public abstract int Compare(T x, T y);

        int IComparer.Compare(object x, object y)
        {
            if (x == null)
                return y == null ? 0 : -1;
            if (y == null)
                return 1;
            if (x is T && y is T)
                return Compare((T)x, (T)y);
            throw new ArgumentException("At least one object must implement IComparable.");
        }
    }

    internal sealed class ComparisonComparer<T> : Comparer<T>
    {
        private readonly Comparison<T> _comparison;

        public ComparisonComparer(Comparison<T> comparison)
        {
            _comparison = comparison;
        }

        public override int Compare(T x, T y)
        {
            return _comparison(x, y);
        }
    }

    internal sealed class ObjectComparer<T> : Comparer<T>
    {
        public override int Compare(T x, T y)
        {
            return Ordre<T>.Compare(x, y);
        }
    }

    internal static class Ordre
    {
        private static int Sign(long a, long b) { return a < b ? -1 : (a > b ? 1 : 0); }
        private static int SignU(ulong a, ulong b) { return a < b ? -1 : (a > b ? 1 : 0); }

        private static int SignD(double a, double b)
        {
            if (a < b) return -1;
            if (a > b) return 1;
            if (a == b) return 0;
            // NaN passe avant tout nombre, comme Double.CompareTo.
            if (double.IsNaN(a)) return double.IsNaN(b) ? 0 : -1;
            return 1;
        }

        public static int Compare(object x, object y)
        {
            if (x == null)
                return y == null ? 0 : -1;
            if (y == null)
                return 1;
            if (x is int && y is int) return Sign((int)x, (int)y);
            if (x is long && y is long) return Sign((long)x, (long)y);
            if (x is short && y is short) return Sign((short)x, (short)y);
            if (x is sbyte && y is sbyte) return Sign((sbyte)x, (sbyte)y);
            if (x is byte && y is byte) return Sign((byte)x, (byte)y);
            if (x is ushort && y is ushort) return Sign((ushort)x, (ushort)y);
            if (x is char && y is char) return Sign((char)x, (char)y);
            if (x is uint && y is uint) return Sign((uint)x, (uint)y);
            if (x is ulong && y is ulong) return SignU((ulong)x, (ulong)y);
            if (x is double && y is double) return SignD((double)x, (double)y);
            if (x is float && y is float) return SignD((float)x, (float)y);
            if (x is bool && y is bool) return Sign((bool)x ? 1 : 0, (bool)y ? 1 : 0);
            string sx = x as string;
            if ((object)sx != null && y is string)
                return string.Compare(sx, (string)y);
            IComparable c = x as IComparable;
            if (c != null)
                return c.CompareTo(y);
            throw new ArgumentException("At least one object must implement IComparable.");
        }
    }

    // Comparateur générique : le passage par object évite de dépendre de
    // l'interface IComparable<T> de chaque primitive.
    internal static class Ordre<T>
    {
        public static int Compare(T x, T y)
        {
            IComparable<T> g = x as IComparable<T>;
            if (g != null && !(x is string))
                return g.CompareTo(y);
            return Ordre.Compare(x, y);
        }
    }
}
