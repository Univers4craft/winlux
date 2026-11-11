// BCL Winlux : System.Array, BitConverter et Guid (#1962). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« Array Class »,
// « BitConverter Class », « Guid Struct ») et la RFC 4122 pour les GUID de
// version 4 ; aucun code de Microsoft, de Mono ou de Wine.
//
// Les opérations sur les octets des éléments (Copy, Clear, Reverse, Clone,
// CreateInstance) sont servies par le moteur (runtime/src/clr/tableaux.rs) ;
// les méthodes génériques s'exécutent ici.

using System.Collections.Generic;
using System.Runtime.CompilerServices;

namespace System
{
    public abstract partial class Array
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Copy(Array sourceArray, Array destinationArray, int length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Copy(Array sourceArray, int sourceIndex, Array destinationArray, int destinationIndex, int length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Copy(Array sourceArray, Array destinationArray, long length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Copy(Array sourceArray, long sourceIndex, Array destinationArray, long destinationIndex, long length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void ConstrainedCopy(Array sourceArray, int sourceIndex, Array destinationArray, int destinationIndex, int length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Clear(Array array, int index, int length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Reverse(Array array);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Reverse(Array array, int index, int length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern Array CreateInstance(Type elementType, int length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern object Clone();

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern void CopyTo(Array array, int index);

        public extern long LongLength
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        public extern int Rank
        {
            [MethodImpl(MethodImplOptions.InternalCall)]
            get;
        }

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern int GetLength(int dimension);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern int GetLowerBound(int dimension);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern int GetUpperBound(int dimension);

        private static void CheckArray(object array)
        {
            if (array == null)
                throw new ArgumentNullException("array");
        }

        public static T[] Empty<T>()
        {
            return new T[0];
        }

        public static void Resize<T>(ref T[] array, int newSize)
        {
            if (newSize < 0)
                throw new ArgumentOutOfRangeException("newSize");
            T[] larger = new T[newSize];
            if (array != null)
                Copy(array, larger, array.Length < newSize ? array.Length : newSize);
            array = larger;
        }

        public static int IndexOf<T>(T[] array, T value)
        {
            CheckArray(array);
            return IndexOf(array, value, 0, array.Length);
        }

        public static int IndexOf<T>(T[] array, T value, int startIndex)
        {
            CheckArray(array);
            return IndexOf(array, value, startIndex, array.Length - startIndex);
        }

        public static int IndexOf<T>(T[] array, T value, int startIndex, int count)
        {
            CheckArray(array);
            if (startIndex < 0 || startIndex > array.Length)
                throw new ArgumentOutOfRangeException("startIndex");
            if (count < 0 || startIndex + count > array.Length)
                throw new ArgumentOutOfRangeException("count");
            EqualityComparer<T> c = EqualityComparer<T>.Default;
            for (int i = startIndex; i < startIndex + count; i++)
            {
                if (c.Equals(array[i], value))
                    return i;
            }
            return -1;
        }

        public static int LastIndexOf<T>(T[] array, T value)
        {
            CheckArray(array);
            EqualityComparer<T> c = EqualityComparer<T>.Default;
            for (int i = array.Length - 1; i >= 0; i--)
            {
                if (c.Equals(array[i], value))
                    return i;
            }
            return -1;
        }

        // IndexOf non générique : égalité d'Object.Equals.
        public static int IndexOf(Array array, object value)
        {
            CheckArray(array);
            object[] items = array as object[];
            if (items == null)
                throw new NotSupportedException("Array.IndexOf(Array, object) : tableau de valeurs non pris en charge.");
            for (int i = 0; i < items.Length; i++)
            {
                if (items[i] == null ? value == null : items[i].Equals(value))
                    return i;
            }
            return -1;
        }

        public static bool Exists<T>(T[] array, Predicate<T> match)
        {
            return FindIndex(array, match) >= 0;
        }

        public static T Find<T>(T[] array, Predicate<T> match)
        {
            int i = FindIndex(array, match);
            return i >= 0 ? array[i] : default(T);
        }

        public static int FindIndex<T>(T[] array, Predicate<T> match)
        {
            CheckArray(array);
            if (match == null)
                throw new ArgumentNullException("match");
            for (int i = 0; i < array.Length; i++)
            {
                if (match(array[i]))
                    return i;
            }
            return -1;
        }

        public static T FindLast<T>(T[] array, Predicate<T> match)
        {
            CheckArray(array);
            if (match == null)
                throw new ArgumentNullException("match");
            for (int i = array.Length - 1; i >= 0; i--)
            {
                if (match(array[i]))
                    return array[i];
            }
            return default(T);
        }

        public static T[] FindAll<T>(T[] array, Predicate<T> match)
        {
            CheckArray(array);
            if (match == null)
                throw new ArgumentNullException("match");
            List<T> found = new List<T>();
            for (int i = 0; i < array.Length; i++)
            {
                if (match(array[i]))
                    found.Add(array[i]);
            }
            return found.ToArray();
        }

        public static bool TrueForAll<T>(T[] array, Predicate<T> match)
        {
            CheckArray(array);
            if (match == null)
                throw new ArgumentNullException("match");
            for (int i = 0; i < array.Length; i++)
            {
                if (!match(array[i]))
                    return false;
            }
            return true;
        }

        public static void ForEach<T>(T[] array, Action<T> action)
        {
            CheckArray(array);
            if (action == null)
                throw new ArgumentNullException("action");
            for (int i = 0; i < array.Length; i++)
                action(array[i]);
        }

        public static TOutput[] ConvertAll<TInput, TOutput>(TInput[] array, Converter<TInput, TOutput> converter)
        {
            CheckArray(array);
            if (converter == null)
                throw new ArgumentNullException("converter");
            TOutput[] result = new TOutput[array.Length];
            for (int i = 0; i < array.Length; i++)
                result[i] = converter(array[i]);
            return result;
        }

        public static void Sort<T>(T[] array)
        {
            Sort(array, 0, array == null ? 0 : array.Length, (IComparer<T>)null);
        }

        public static void Sort<T>(T[] array, IComparer<T> comparer)
        {
            Sort(array, 0, array == null ? 0 : array.Length, comparer);
        }

        public static void Sort<T>(T[] array, Comparison<T> comparison)
        {
            if (comparison == null)
                throw new ArgumentNullException("comparison");
            Sort(array, 0, array == null ? 0 : array.Length, Comparer<T>.Create(comparison));
        }

        // Tri par insertion, stable, comme List<T>.Sort.
        public static void Sort<T>(T[] array, int index, int length, IComparer<T> comparer)
        {
            CheckArray(array);
            if (index < 0 || length < 0 || index + length > array.Length)
                throw new ArgumentOutOfRangeException("length");
            IComparer<T> c = comparer != null ? comparer : Comparer<T>.Default;
            for (int i = index + 1; i < index + length; i++)
            {
                T item = array[i];
                int j = i - 1;
                while (j >= index && c.Compare(array[j], item) > 0)
                {
                    array[j + 1] = array[j];
                    j--;
                }
                array[j + 1] = item;
            }
        }

        public static int BinarySearch<T>(T[] array, T value)
        {
            return BinarySearch(array, value, null);
        }

        public static int BinarySearch<T>(T[] array, T value, IComparer<T> comparer)
        {
            CheckArray(array);
            IComparer<T> c = comparer != null ? comparer : Comparer<T>.Default;
            int lo = 0;
            int hi = array.Length - 1;
            while (lo <= hi)
            {
                int mid = lo + ((hi - lo) >> 1);
                int order = c.Compare(array[mid], value);
                if (order == 0)
                    return mid;
                if (order < 0)
                    lo = mid + 1;
                else
                    hi = mid - 1;
            }
            return ~lo;
        }
    }

    public delegate TOutput Converter<in TInput, out TOutput>(TInput input);

    // Ordre des octets de la machine : petit-boutiste sur x86 et x64.
    public static class BitConverter
    {
        public static readonly bool IsLittleEndian = true;

        private static byte[] Bytes(ulong value, int size)
        {
            byte[] b = new byte[size];
            for (int i = 0; i < size; i++)
                b[i] = (byte)(value >> (8 * i));
            return b;
        }

        public static byte[] GetBytes(bool value) { return new byte[] { value ? (byte)1 : (byte)0 }; }
        public static byte[] GetBytes(char value) { return Bytes(value, 2); }
        public static byte[] GetBytes(short value) { return Bytes((ulong)value, 2); }
        public static byte[] GetBytes(ushort value) { return Bytes(value, 2); }
        public static byte[] GetBytes(int value) { return Bytes((ulong)value, 4); }
        public static byte[] GetBytes(uint value) { return Bytes(value, 4); }
        public static byte[] GetBytes(long value) { return Bytes((ulong)value, 8); }
        public static byte[] GetBytes(ulong value) { return Bytes(value, 8); }
        public static byte[] GetBytes(float value) { return Bytes((uint)SingleToInt32Bits(value), 4); }
        public static byte[] GetBytes(double value) { return Bytes((ulong)DoubleToInt64Bits(value), 8); }

        private static ulong Read(byte[] value, int startIndex, int size)
        {
            if (value == null)
                throw new ArgumentNullException("value");
            if ((uint)startIndex >= (uint)value.Length)
                throw new ArgumentOutOfRangeException("startIndex");
            if (startIndex > value.Length - size)
                throw new ArgumentException("Destination array is not long enough to copy all the items in the collection. Check array index and length.");
            ulong r = 0;
            for (int i = size - 1; i >= 0; i--)
                r = (r << 8) | value[startIndex + i];
            return r;
        }

        public static bool ToBoolean(byte[] value, int startIndex) { return Read(value, startIndex, 1) != 0; }
        public static char ToChar(byte[] value, int startIndex) { return (char)Read(value, startIndex, 2); }
        public static short ToInt16(byte[] value, int startIndex) { return (short)Read(value, startIndex, 2); }
        public static ushort ToUInt16(byte[] value, int startIndex) { return (ushort)Read(value, startIndex, 2); }
        public static int ToInt32(byte[] value, int startIndex) { return (int)Read(value, startIndex, 4); }
        public static uint ToUInt32(byte[] value, int startIndex) { return (uint)Read(value, startIndex, 4); }
        public static long ToInt64(byte[] value, int startIndex) { return (long)Read(value, startIndex, 8); }
        public static ulong ToUInt64(byte[] value, int startIndex) { return Read(value, startIndex, 8); }
        public static float ToSingle(byte[] value, int startIndex) { return Int32BitsToSingle((int)Read(value, startIndex, 4)); }
        public static double ToDouble(byte[] value, int startIndex) { return Int64BitsToDouble((long)Read(value, startIndex, 8)); }

        public static unsafe long DoubleToInt64Bits(double value) { return *(long*)&value; }
        public static unsafe double Int64BitsToDouble(long value) { return *(double*)&value; }
        public static unsafe int SingleToInt32Bits(float value) { return *(int*)&value; }
        public static unsafe float Int32BitsToSingle(int value) { return *(float*)&value; }

        public static string ToString(byte[] value)
        {
            if (value == null)
                throw new ArgumentNullException("value");
            return ToString(value, 0, value.Length);
        }

        public static string ToString(byte[] value, int startIndex)
        {
            if (value == null)
                throw new ArgumentNullException("value");
            return ToString(value, startIndex, value.Length - startIndex);
        }

        // « 0A-1B-FF ».
        public static string ToString(byte[] value, int startIndex, int length)
        {
            if (value == null)
                throw new ArgumentNullException("value");
            if (startIndex < 0 || length < 0 || startIndex > value.Length - length)
                throw new ArgumentOutOfRangeException("length");
            const string hex = "0123456789ABCDEF";
            char[] chars = new char[length == 0 ? 0 : length * 3 - 1];
            for (int i = 0; i < length; i++)
            {
                byte b = value[startIndex + i];
                chars[i * 3] = hex[b >> 4];
                chars[i * 3 + 1] = hex[b & 15];
                if (i < length - 1)
                    chars[i * 3 + 2] = '-';
            }
            return new string(chars);
        }
    }

    public struct Guid : IComparable, IComparable<Guid>, IEquatable<Guid>, IFormattable
    {
        public static readonly Guid Empty = new Guid();

        private int _a;
        private short _b;
        private short _c;
        private byte _d, _e, _f, _g, _h, _i, _j, _k;

        public Guid(byte[] b)
        {
            if (b == null)
                throw new ArgumentNullException("b");
            if (b.Length != 16)
                throw new ArgumentException("Byte array for GUID must be exactly 16 bytes long.");
            _a = b[0] | (b[1] << 8) | (b[2] << 16) | (b[3] << 24);
            _b = (short)(b[4] | (b[5] << 8));
            _c = (short)(b[6] | (b[7] << 8));
            _d = b[8]; _e = b[9]; _f = b[10]; _g = b[11];
            _h = b[12]; _i = b[13]; _j = b[14]; _k = b[15];
        }

        public Guid(int a, short b, short c, byte d, byte e, byte f, byte g, byte h, byte i, byte j, byte k)
        {
            _a = a; _b = b; _c = c;
            _d = d; _e = e; _f = f; _g = g; _h = h; _i = i; _j = j; _k = k;
        }

        public Guid(int a, short b, short c, byte[] d)
        {
            if (d == null)
                throw new ArgumentNullException("d");
            if (d.Length != 8)
                throw new ArgumentException("Byte array for GUID must be exactly 8 bytes long.");
            _a = a; _b = b; _c = c;
            _d = d[0]; _e = d[1]; _f = d[2]; _g = d[3]; _h = d[4]; _i = d[5]; _j = d[6]; _k = d[7];
        }

        public Guid(string g)
        {
            this = Parse(g);
        }

        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern void FillRandom(byte[] buffer);

        // Version 4 (aléatoire) de la RFC 4122.
        public static Guid NewGuid()
        {
            byte[] b = new byte[16];
            FillRandom(b);
            b[7] = (byte)((b[7] & 0x0F) | 0x40);
            b[8] = (byte)((b[8] & 0x3F) | 0x80);
            return new Guid(b);
        }

        public byte[] ToByteArray()
        {
            return new byte[]
            {
                (byte)_a, (byte)(_a >> 8), (byte)(_a >> 16), (byte)(_a >> 24),
                (byte)_b, (byte)(_b >> 8), (byte)_c, (byte)(_c >> 8),
                _d, _e, _f, _g, _h, _i, _j, _k,
            };
        }

        public override string ToString()
        {
            return ToString("D");
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        // N : 32 chiffres ; D : avec tirets ; B : entre accolades ;
        // P : entre parenthèses.
        public string ToString(string format)
        {
            if ((object)format == null || format.Length == 0)
                format = "D";
            if (format.Length != 1)
                throw new FormatException("Format String can be only \"D\", \"d\", \"N\", \"n\", \"P\", \"p\", \"B\", \"b\", \"X\" or \"x\".");
            char f = format[0];
            string digits = string.Concat(_a.ToString("x8"), ((ushort)_b).ToString("x4"), ((ushort)_c).ToString("x4"),
                string.Concat(_d.ToString("x2"), _e.ToString("x2"), _f.ToString("x2"), _g.ToString("x2")),
                string.Concat(_h.ToString("x2"), _i.ToString("x2"), _j.ToString("x2"), _k.ToString("x2")));
            if (f == 'N' || f == 'n')
                return digits;
            string dashed = string.Concat(
                string.Concat(digits.Substring(0, 8), "-", digits.Substring(8, 4), "-"),
                string.Concat(digits.Substring(12, 4), "-", digits.Substring(16, 4), "-"),
                digits.Substring(20));
            if (f == 'D' || f == 'd')
                return dashed;
            if (f == 'B' || f == 'b')
                return string.Concat("{", dashed, "}");
            if (f == 'P' || f == 'p')
                return string.Concat("(", dashed, ")");
            throw new FormatException("Format String can be only \"D\", \"d\", \"N\", \"n\", \"P\", \"p\", \"B\", \"b\", \"X\" or \"x\".");
        }

        private static int Hex(char c)
        {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            if (c >= 'A' && c <= 'F') return c - 'A' + 10;
            return -1;
        }

        public static bool TryParse(string input, out Guid result)
        {
            result = Empty;
            if ((object)input == null)
                return false;
            string s = input.Trim();
            if (s.Length == 38 && ((s[0] == '{' && s[37] == '}') || (s[0] == '(' && s[37] == ')')))
                s = s.Substring(1, 36);
            if (s.Length == 36)
            {
                if (s[8] != '-' || s[13] != '-' || s[18] != '-' || s[23] != '-')
                    return false;
                s = string.Concat(string.Concat(s.Substring(0, 8), s.Substring(9, 4), s.Substring(14, 4)),
                    s.Substring(19, 4), s.Substring(24));
            }
            if (s.Length != 32)
                return false;
            byte[] v = new byte[16];
            for (int i = 0; i < 16; i++)
            {
                int hi = Hex(s[2 * i]);
                int lo = Hex(s[2 * i + 1]);
                if (hi < 0 || lo < 0)
                    return false;
                v[i] = (byte)((hi << 4) | lo);
            }
            // Le texte donne a, b et c de poids fort en premier.
            result = new Guid((v[0] << 24) | (v[1] << 16) | (v[2] << 8) | v[3],
                (short)((v[4] << 8) | v[5]), (short)((v[6] << 8) | v[7]),
                v[8], v[9], v[10], v[11], v[12], v[13], v[14], v[15]);
            return true;
        }

        public static Guid Parse(string input)
        {
            if ((object)input == null)
                throw new ArgumentNullException("input");
            Guid g;
            if (!TryParse(input, out g))
                throw new FormatException("Guid should contain 32 digits with 4 dashes (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx).");
            return g;
        }

        public bool Equals(Guid g)
        {
            return _a == g._a && _b == g._b && _c == g._c && _d == g._d && _e == g._e && _f == g._f
                && _g == g._g && _h == g._h && _i == g._i && _j == g._j && _k == g._k;
        }

        public override bool Equals(object o)
        {
            return o is Guid && Equals((Guid)o);
        }

        public override int GetHashCode()
        {
            return _a ^ ((_b << 16) | (ushort)_c) ^ ((_f << 24) | _k);
        }

        public int CompareTo(Guid value)
        {
            byte[] x = ToByteArray();
            byte[] y = value.ToByteArray();
            if (_a != value._a) return (uint)_a < (uint)value._a ? -1 : 1;
            if (_b != value._b) return (ushort)_b < (ushort)value._b ? -1 : 1;
            if (_c != value._c) return (ushort)_c < (ushort)value._c ? -1 : 1;
            for (int i = 8; i < 16; i++)
            {
                if (x[i] != y[i])
                    return x[i] < y[i] ? -1 : 1;
            }
            return 0;
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is Guid))
                throw new ArgumentException("Object must be of type GUID.");
            return CompareTo((Guid)value);
        }

        public static bool operator ==(Guid a, Guid b) { return a.Equals(b); }
        public static bool operator !=(Guid a, Guid b) { return !a.Equals(b); }
    }
}
