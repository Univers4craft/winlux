// BCL Winlux : membres de System.String, StringComparison,
// StringSplitOptions et System.Text.StringBuilder (#1956). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« String Class »,
// « Composite formatting », « StringBuilder Class ») ; aucun code de
// Microsoft, de Mono ou de Wine.
//
// Les indices comptent des unités UTF-16, comme .NET. Le moteur sert la
// longueur, l'indexeur, les constructeurs, Substring, Replace(string,
// string) et ToUpper/ToLower ; le reste s'exécute ici. Les comparaisons
// « culturelles » suivent un ordre invariant simple : sans tenir compte de
// la casse d'abord, puis minuscule avant majuscule à égalité.

using System.Collections.Generic;
using System.Runtime.CompilerServices;
using System.Text;

namespace System
{
    public enum StringComparison
    {
        CurrentCulture = 0,
        CurrentCultureIgnoreCase = 1,
        InvariantCulture = 2,
        InvariantCultureIgnoreCase = 3,
        Ordinal = 4,
        OrdinalIgnoreCase = 5,
    }

    public sealed class IndexOutOfRangeException : SystemException
    {
        public IndexOutOfRangeException() : base("Index was outside the bounds of the array.") { }
        public IndexOutOfRangeException(string message) : base(message) { }
    }

    [Flags]
    public enum StringSplitOptions
    {
        None = 0,
        RemoveEmptyEntries = 1,
    }

    public sealed partial class String : IComparable, IComparable<string>, IEquatable<string>
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern String(char[] value);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern String(char[] value, int startIndex, int length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern String(char c, int count);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern string Substring(int startIndex);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern string Substring(int startIndex, int length);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern string Replace(string oldValue, string newValue);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern string ToUpper();

        [MethodImpl(MethodImplOptions.InternalCall)]
        public extern string ToLower();

        public string ToUpperInvariant() { return ToUpper(); }
        public string ToLowerInvariant() { return ToLower(); }
        public override string ToString() { return this; }
        public string ToString(IFormatProvider provider) { return this; }

        // ── Égalité et comparaison ──

        private static char Fold(char c)
        {
            if (c >= 'a' && c <= 'z')
                return (char)(c - 32);
            if (c < 0x80)
                return c;
            return Char.ToUpper(c);
        }

        private static int CompareCore(string a, string b, bool ignoreCase)
        {
            int n = a.Length < b.Length ? a.Length : b.Length;
            for (int i = 0; i < n; i++)
            {
                char x = a[i];
                char y = b[i];
                if (ignoreCase)
                {
                    x = Fold(x);
                    y = Fold(y);
                }
                if (x != y)
                    return x < y ? -1 : 1;
            }
            return a.Length == b.Length ? 0 : (a.Length < b.Length ? -1 : 1);
        }

        private static bool IgnoresCase(StringComparison comparisonType)
        {
            return comparisonType == StringComparison.CurrentCultureIgnoreCase
                || comparisonType == StringComparison.InvariantCultureIgnoreCase
                || comparisonType == StringComparison.OrdinalIgnoreCase;
        }

        public static int CompareOrdinal(string strA, string strB)
        {
            if ((object)strA == null)
                return (object)strB == null ? 0 : -1;
            if ((object)strB == null)
                return 1;
            return CompareCore(strA, strB, false);
        }

        public static int Compare(string strA, string strB)
        {
            return Compare(strA, strB, StringComparison.CurrentCulture);
        }

        public static int Compare(string strA, string strB, bool ignoreCase)
        {
            return Compare(strA, strB, ignoreCase ? StringComparison.CurrentCultureIgnoreCase : StringComparison.CurrentCulture);
        }

        public static int Compare(string strA, string strB, StringComparison comparisonType)
        {
            if ((object)strA == null)
                return (object)strB == null ? 0 : -1;
            if ((object)strB == null)
                return 1;
            if (comparisonType == StringComparison.Ordinal)
                return CompareCore(strA, strB, false);
            int folded = CompareCore(strA, strB, true);
            if (folded != 0 || IgnoresCase(comparisonType))
                return folded;
            // À égalité sans casse, la minuscule passe d'abord.
            return -CompareCore(strA, strB, false);
        }

        public static int Compare(string strA, int indexA, string strB, int indexB, int length)
        {
            return Compare(Part(strA, indexA, length), Part(strB, indexB, length), StringComparison.CurrentCulture);
        }

        public static int Compare(string strA, int indexA, string strB, int indexB, int length, StringComparison comparisonType)
        {
            return Compare(Part(strA, indexA, length), Part(strB, indexB, length), comparisonType);
        }

        private static string Part(string s, int index, int length)
        {
            if ((object)s == null)
                return null;
            int n = s.Length - index < length ? s.Length - index : length;
            return s.Substring(index, n < 0 ? 0 : n);
        }

        public int CompareTo(string strB)
        {
            return Compare(this, strB);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            string s = value as string;
            if ((object)s == null)
                throw new ArgumentException("Object must be of type String.");
            return Compare(this, s);
        }

        public bool Equals(string value)
        {
            return this == value;
        }

        public bool Equals(string value, StringComparison comparisonType)
        {
            return Equals(this, value, comparisonType);
        }

        public static bool Equals(string a, string b)
        {
            return a == b;
        }

        public static bool Equals(string a, string b, StringComparison comparisonType)
        {
            if ((object)a == null || (object)b == null)
                return (object)a == (object)b;
            if (a.Length != b.Length)
                return false;
            return CompareCore(a, b, IgnoresCase(comparisonType)) == 0;
        }

        public bool StartsWith(string value, StringComparison comparisonType)
        {
            if ((object)value == null)
                throw new ArgumentNullException("value");
            return value.Length <= Length && Equals(Substring(0, value.Length), value, comparisonType);
        }

        public bool StartsWith(string value, bool ignoreCase, IFormatProvider culture)
        {
            return StartsWith(value, ignoreCase ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal);
        }

        public bool StartsWith(char value)
        {
            return Length > 0 && this[0] == value;
        }

        public bool EndsWith(string value, StringComparison comparisonType)
        {
            if ((object)value == null)
                throw new ArgumentNullException("value");
            return value.Length <= Length && Equals(Substring(Length - value.Length), value, comparisonType);
        }

        public bool EndsWith(char value)
        {
            return Length > 0 && this[Length - 1] == value;
        }

        // ── Recherche ──

        public bool Contains(string value)
        {
            if ((object)value == null)
                throw new ArgumentNullException("value");
            return IndexOf(value, 0, Length, StringComparison.Ordinal) >= 0;
        }

        public bool Contains(char value)
        {
            return IndexOf(value) >= 0;
        }

        public int IndexOf(char value)
        {
            return IndexOf(value, 0, Length);
        }

        public int IndexOf(char value, int startIndex)
        {
            return IndexOf(value, startIndex, Length - startIndex);
        }

        public int IndexOf(char value, int startIndex, int count)
        {
            CheckRange(startIndex, count);
            for (int i = startIndex; i < startIndex + count; i++)
            {
                if (this[i] == value)
                    return i;
            }
            return -1;
        }

        public int IndexOf(string value)
        {
            return IndexOf(value, 0, Length, StringComparison.CurrentCulture);
        }

        public int IndexOf(string value, int startIndex)
        {
            return IndexOf(value, startIndex, Length - startIndex, StringComparison.CurrentCulture);
        }

        public int IndexOf(string value, int startIndex, int count)
        {
            return IndexOf(value, startIndex, count, StringComparison.CurrentCulture);
        }

        public int IndexOf(string value, StringComparison comparisonType)
        {
            return IndexOf(value, 0, Length, comparisonType);
        }

        public int IndexOf(string value, int startIndex, StringComparison comparisonType)
        {
            return IndexOf(value, startIndex, Length - startIndex, comparisonType);
        }

        public int IndexOf(string value, int startIndex, int count, StringComparison comparisonType)
        {
            if ((object)value == null)
                throw new ArgumentNullException("value");
            CheckRange(startIndex, count);
            bool ignoreCase = IgnoresCase(comparisonType);
            int last = startIndex + count - value.Length;
            for (int i = startIndex; i <= last; i++)
            {
                if (MatchAt(i, value, ignoreCase))
                    return i;
            }
            return -1;
        }

        private bool MatchAt(int index, string value, bool ignoreCase)
        {
            for (int j = 0; j < value.Length; j++)
            {
                char x = this[index + j];
                char y = value[j];
                if (x != y && (!ignoreCase || Fold(x) != Fold(y)))
                    return false;
            }
            return true;
        }

        private void CheckRange(int startIndex, int count)
        {
            if (startIndex < 0 || startIndex > Length)
                throw new ArgumentOutOfRangeException("startIndex");
            if (count < 0 || startIndex + count > Length)
                throw new ArgumentOutOfRangeException("count");
        }

        public int IndexOfAny(char[] anyOf)
        {
            return IndexOfAny(anyOf, 0, Length);
        }

        public int IndexOfAny(char[] anyOf, int startIndex)
        {
            return IndexOfAny(anyOf, startIndex, Length - startIndex);
        }

        public int IndexOfAny(char[] anyOf, int startIndex, int count)
        {
            if (anyOf == null)
                throw new ArgumentNullException("anyOf");
            CheckRange(startIndex, count);
            for (int i = startIndex; i < startIndex + count; i++)
            {
                if (In(this[i], anyOf))
                    return i;
            }
            return -1;
        }

        private static bool In(char c, char[] set)
        {
            for (int k = 0; k < set.Length; k++)
            {
                if (set[k] == c)
                    return true;
            }
            return false;
        }

        public int LastIndexOf(char value)
        {
            for (int i = Length - 1; i >= 0; i--)
            {
                if (this[i] == value)
                    return i;
            }
            return -1;
        }

        public int LastIndexOf(char value, int startIndex)
        {
            if (Length == 0)
                return -1;
            if (startIndex < 0 || startIndex >= Length)
                throw new ArgumentOutOfRangeException("startIndex");
            for (int i = startIndex; i >= 0; i--)
            {
                if (this[i] == value)
                    return i;
            }
            return -1;
        }

        public int LastIndexOf(string value)
        {
            return LastIndexOf(value, StringComparison.CurrentCulture);
        }

        public int LastIndexOf(string value, StringComparison comparisonType)
        {
            if ((object)value == null)
                throw new ArgumentNullException("value");
            bool ignoreCase = IgnoresCase(comparisonType);
            for (int i = Length - value.Length; i >= 0; i--)
            {
                if (MatchAt(i, value, ignoreCase))
                    return i;
            }
            return -1;
        }

        public int LastIndexOfAny(char[] anyOf)
        {
            if (anyOf == null)
                throw new ArgumentNullException("anyOf");
            for (int i = Length - 1; i >= 0; i--)
            {
                if (In(this[i], anyOf))
                    return i;
            }
            return -1;
        }

        // ── Transformation ──

        public char[] ToCharArray()
        {
            return ToCharArray(0, Length);
        }

        public char[] ToCharArray(int startIndex, int length)
        {
            CheckRange(startIndex, length);
            char[] result = new char[length];
            for (int i = 0; i < length; i++)
                result[i] = this[startIndex + i];
            return result;
        }

        public void CopyTo(int sourceIndex, char[] destination, int destinationIndex, int count)
        {
            if (destination == null)
                throw new ArgumentNullException("destination");
            CheckRange(sourceIndex, count);
            for (int i = 0; i < count; i++)
                destination[destinationIndex + i] = this[sourceIndex + i];
        }

        public string Trim()
        {
            return TrimCore(null, true, true);
        }

        public string Trim(params char[] trimChars)
        {
            return TrimCore(trimChars, true, true);
        }

        public string Trim(char trimChar)
        {
            return TrimCore(new char[] { trimChar }, true, true);
        }

        public string TrimStart(params char[] trimChars)
        {
            return TrimCore(trimChars, true, false);
        }

        public string TrimEnd(params char[] trimChars)
        {
            return TrimCore(trimChars, false, true);
        }

        // Sans liste (null ou vide) : les espaces Unicode.
        private string TrimCore(char[] set, bool start, bool end)
        {
            bool spaces = set == null || set.Length == 0;
            int i = 0;
            int j = Length - 1;
            if (start)
                while (i <= j && (spaces ? Char.IsWhiteSpace(this[i]) : In(this[i], set))) i++;
            if (end)
                while (j >= i && (spaces ? Char.IsWhiteSpace(this[j]) : In(this[j], set))) j--;
            if (i == 0 && j == Length - 1)
                return this;
            return Substring(i, j - i + 1);
        }

        public string PadLeft(int totalWidth)
        {
            return PadLeft(totalWidth, ' ');
        }

        public string PadLeft(int totalWidth, char paddingChar)
        {
            if (totalWidth < 0)
                throw new ArgumentOutOfRangeException("totalWidth");
            if (totalWidth <= Length)
                return this;
            return Concat(new string(paddingChar, totalWidth - Length), this);
        }

        public string PadRight(int totalWidth)
        {
            return PadRight(totalWidth, ' ');
        }

        public string PadRight(int totalWidth, char paddingChar)
        {
            if (totalWidth < 0)
                throw new ArgumentOutOfRangeException("totalWidth");
            if (totalWidth <= Length)
                return this;
            return Concat(this, new string(paddingChar, totalWidth - Length));
        }

        public string Insert(int startIndex, string value)
        {
            if ((object)value == null)
                throw new ArgumentNullException("value");
            if (startIndex < 0 || startIndex > Length)
                throw new ArgumentOutOfRangeException("startIndex");
            return Concat(Substring(0, startIndex), value, Substring(startIndex));
        }

        public string Remove(int startIndex)
        {
            if (startIndex < 0 || startIndex > Length)
                throw new ArgumentOutOfRangeException("startIndex");
            return Substring(0, startIndex);
        }

        public string Remove(int startIndex, int count)
        {
            CheckRange(startIndex, count);
            return Concat(Substring(0, startIndex), Substring(startIndex + count));
        }

        public string Replace(char oldChar, char newChar)
        {
            char[] chars = ToCharArray();
            bool changed = false;
            for (int i = 0; i < chars.Length; i++)
            {
                if (chars[i] == oldChar)
                {
                    chars[i] = newChar;
                    changed = true;
                }
            }
            return changed ? new string(chars) : this;
        }

        // ── Découpage et assemblage ──

        public string[] Split(params char[] separator)
        {
            return Split(separator, int.MaxValue, StringSplitOptions.None);
        }

        public string[] Split(char[] separator, int count)
        {
            return Split(separator, count, StringSplitOptions.None);
        }

        public string[] Split(char[] separator, StringSplitOptions options)
        {
            return Split(separator, int.MaxValue, options);
        }

        public string[] Split(char separator, StringSplitOptions options)
        {
            return Split(new char[] { separator }, int.MaxValue, options);
        }

        public string[] Split(char[] separator, int count, StringSplitOptions options)
        {
            if (count < 0)
                throw new ArgumentOutOfRangeException("count");
            bool spaces = separator == null || separator.Length == 0;
            List<int> starts = new List<int>();
            List<int> lengths = new List<int>();
            int from = 0;
            for (int i = 0; i < Length; i++)
            {
                char c = this[i];
                if (spaces ? Char.IsWhiteSpace(c) : In(c, separator))
                {
                    starts.Add(from);
                    lengths.Add(i - from);
                    from = i + 1;
                }
            }
            starts.Add(from);
            lengths.Add(Length - from);
            return Pieces(starts, lengths, count, options);
        }

        public string[] Split(string[] separator, StringSplitOptions options)
        {
            return Split(separator, int.MaxValue, options);
        }

        public string[] Split(string separator, StringSplitOptions options)
        {
            return Split(new string[] { separator }, int.MaxValue, options);
        }

        public string[] Split(string[] separator, int count, StringSplitOptions options)
        {
            if (count < 0)
                throw new ArgumentOutOfRangeException("count");
            if (separator == null || separator.Length == 0)
                return Split((char[])null, count, options);
            List<int> starts = new List<int>();
            List<int> lengths = new List<int>();
            int from = 0;
            int i = 0;
            while (i < Length)
            {
                int matched = -1;
                for (int k = 0; k < separator.Length; k++)
                {
                    string sep = separator[k];
                    if ((object)sep != null && sep.Length > 0 && sep.Length <= Length - i && MatchAt(i, sep, false))
                    {
                        matched = sep.Length;
                        break;
                    }
                }
                if (matched > 0)
                {
                    starts.Add(from);
                    lengths.Add(i - from);
                    i += matched;
                    from = i;
                }
                else
                {
                    i++;
                }
            }
            starts.Add(from);
            lengths.Add(Length - from);
            return Pieces(starts, lengths, count, options);
        }

        // Le dernier morceau gardé reçoit le reste de la chaîne quand
        // `count` limite le nombre de morceaux.
        private string[] Pieces(List<int> starts, List<int> lengths, int count, StringSplitOptions options)
        {
            bool removeEmpty = (options & StringSplitOptions.RemoveEmptyEntries) != 0;
            List<string> result = new List<string>();
            if (count == 0)
                return result.ToArray();
            for (int k = 0; k < starts.Count; k++)
            {
                if (removeEmpty && lengths[k] == 0)
                    continue;
                if (result.Count == count - 1)
                {
                    result.Add(Substring(starts[k]));
                    break;
                }
                result.Add(Substring(starts[k], lengths[k]));
            }
            return result.ToArray();
        }

        public static string Join(string separator, params string[] value)
        {
            if (value == null)
                throw new ArgumentNullException("value");
            return Join(separator, value, 0, value.Length);
        }

        public static string Join(string separator, string[] value, int startIndex, int count)
        {
            if (value == null)
                throw new ArgumentNullException("value");
            if (startIndex < 0 || count < 0 || startIndex + count > value.Length)
                throw new ArgumentOutOfRangeException("startIndex");
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < count; i++)
            {
                if (i > 0)
                    sb.Append(separator);
                sb.Append(value[startIndex + i]);
            }
            return sb.ToString();
        }

        public static string Join(string separator, params object[] values)
        {
            if (values == null)
                throw new ArgumentNullException("values");
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < values.Length; i++)
            {
                if (i > 0)
                    sb.Append(separator);
                if (values[i] != null)
                    sb.Append(values[i].ToString());
            }
            return sb.ToString();
        }

        public static string Join(string separator, IEnumerable<string> values)
        {
            if (values == null)
                throw new ArgumentNullException("values");
            StringBuilder sb = new StringBuilder();
            bool first = true;
            foreach (string v in values)
            {
                if (!first)
                    sb.Append(separator);
                sb.Append(v);
                first = false;
            }
            return sb.ToString();
        }

        public static string Join<T>(string separator, IEnumerable<T> values)
        {
            if (values == null)
                throw new ArgumentNullException("values");
            StringBuilder sb = new StringBuilder();
            bool first = true;
            foreach (T v in values)
            {
                if (!first)
                    sb.Append(separator);
                if (v != null)
                    sb.Append(v.ToString());
                first = false;
            }
            return sb.ToString();
        }

        public static string Concat(object arg0)
        {
            return arg0 == null ? Empty : arg0.ToString();
        }

        public static string Concat(object arg0, object arg1)
        {
            return Concat(Concat(arg0), Concat(arg1));
        }

        public static string Concat(params object[] args)
        {
            if (args == null)
                throw new ArgumentNullException("args");
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < args.Length; i++)
            {
                if (args[i] != null)
                    sb.Append(args[i].ToString());
            }
            return sb.ToString();
        }

        public static string Concat(params string[] values)
        {
            if (values == null)
                throw new ArgumentNullException("values");
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < values.Length; i++)
                sb.Append(values[i]);
            return sb.ToString();
        }

        public static string Concat(IEnumerable<string> values)
        {
            return Join(Empty, values);
        }

        public static string Copy(string str)
        {
            if ((object)str == null)
                throw new ArgumentNullException("str");
            return new string(str.ToCharArray());
        }

        // ── Mise en forme composite ──

        public static string Format(string format, object arg0)
        {
            return Format(null, format, new object[] { arg0 });
        }

        public static string Format(string format, object arg0, object arg1)
        {
            return Format(null, format, new object[] { arg0, arg1 });
        }

        public static string Format(string format, object arg0, object arg1, object arg2)
        {
            return Format(null, format, new object[] { arg0, arg1, arg2 });
        }

        public static string Format(string format, params object[] args)
        {
            return Format(null, format, args);
        }

        public static string Format(IFormatProvider provider, string format, object arg0)
        {
            return Format(provider, format, new object[] { arg0 });
        }

        public static string Format(IFormatProvider provider, string format, object arg0, object arg1)
        {
            return Format(provider, format, new object[] { arg0, arg1 });
        }

        public static string Format(IFormatProvider provider, string format, params object[] args)
        {
            if ((object)format == null)
                throw new ArgumentNullException("format");
            if (args == null)
                throw new ArgumentNullException("args");
            return new StringBuilder().AppendFormat(provider, format, args).ToString();
        }
    }
}

namespace System.Text
{
    public sealed class StringBuilder
    {
        private char[] _chars;
        private int _length;

        public StringBuilder() : this(16) { }

        public StringBuilder(int capacity)
        {
            if (capacity < 0)
                throw new ArgumentOutOfRangeException("capacity");
            _chars = new char[capacity];
        }

        public StringBuilder(string value) : this(value, 16) { }

        public StringBuilder(string value, int capacity) : this(capacity)
        {
            Append(value);
        }

        public StringBuilder(int capacity, int maxCapacity) : this(capacity) { }

        public int Length
        {
            get { return _length; }
            set
            {
                if (value < 0)
                    throw new ArgumentOutOfRangeException("value");
                EnsureCapacity(value);
                for (int i = _length; i < value; i++)
                    _chars[i] = '\0';
                _length = value;
            }
        }

        public int Capacity
        {
            get { return _chars.Length; }
            set
            {
                if (value < _length)
                    throw new ArgumentOutOfRangeException("value");
                Resize(value);
            }
        }

        public int MaxCapacity { get { return int.MaxValue; } }

        [IndexerName("Chars")]
        public char this[int index]
        {
            get
            {
                if ((uint)index >= (uint)_length)
                    throw new IndexOutOfRangeException();
                return _chars[index];
            }
            set
            {
                if ((uint)index >= (uint)_length)
                    throw new ArgumentOutOfRangeException("index");
                _chars[index] = value;
            }
        }

        public int EnsureCapacity(int capacity)
        {
            if (capacity > _chars.Length)
            {
                int size = _chars.Length * 2;
                Resize(size < capacity ? capacity : size);
            }
            return _chars.Length;
        }

        private void Resize(int size)
        {
            char[] chars = new char[size];
            for (int i = 0; i < _length; i++)
                chars[i] = _chars[i];
            _chars = chars;
        }

        public StringBuilder Clear()
        {
            _length = 0;
            return this;
        }

        public StringBuilder Append(string value)
        {
            if ((object)value == null)
                return this;
            int n = value.Length;
            EnsureCapacity(_length + n);
            for (int i = 0; i < n; i++)
                _chars[_length + i] = value[i];
            _length += n;
            return this;
        }

        public StringBuilder Append(string value, int startIndex, int count)
        {
            if ((object)value == null)
            {
                if (startIndex == 0 && count == 0)
                    return this;
                throw new ArgumentNullException("value");
            }
            return Append(value.Substring(startIndex, count));
        }

        public StringBuilder Append(char value)
        {
            EnsureCapacity(_length + 1);
            _chars[_length] = value;
            _length++;
            return this;
        }

        public StringBuilder Append(char value, int repeatCount)
        {
            if (repeatCount < 0)
                throw new ArgumentOutOfRangeException("repeatCount");
            EnsureCapacity(_length + repeatCount);
            for (int i = 0; i < repeatCount; i++)
                _chars[_length + i] = value;
            _length += repeatCount;
            return this;
        }

        public StringBuilder Append(char[] value)
        {
            if (value == null)
                return this;
            return Append(value, 0, value.Length);
        }

        public StringBuilder Append(char[] value, int startIndex, int charCount)
        {
            if (value == null)
            {
                if (startIndex == 0 && charCount == 0)
                    return this;
                throw new ArgumentNullException("value");
            }
            if (startIndex < 0 || charCount < 0 || startIndex + charCount > value.Length)
                throw new ArgumentOutOfRangeException("startIndex");
            EnsureCapacity(_length + charCount);
            for (int i = 0; i < charCount; i++)
                _chars[_length + i] = value[startIndex + i];
            _length += charCount;
            return this;
        }

        public StringBuilder Append(object value)
        {
            return value == null ? this : Append(value.ToString());
        }

        public StringBuilder Append(bool value) { return Append(value.ToString()); }
        public StringBuilder Append(sbyte value) { return Append(value.ToString()); }
        public StringBuilder Append(byte value) { return Append(value.ToString()); }
        public StringBuilder Append(short value) { return Append(value.ToString()); }
        public StringBuilder Append(ushort value) { return Append(value.ToString()); }
        public StringBuilder Append(int value) { return Append(value.ToString()); }
        public StringBuilder Append(uint value) { return Append(value.ToString()); }
        public StringBuilder Append(long value) { return Append(value.ToString()); }
        public StringBuilder Append(ulong value) { return Append(value.ToString()); }
        public StringBuilder Append(float value) { return Append(value.ToString()); }
        public StringBuilder Append(double value) { return Append(value.ToString()); }

        public StringBuilder Append(StringBuilder value)
        {
            return value == null ? this : Append(value.ToString());
        }

        public StringBuilder AppendLine()
        {
            return Append("\r\n");
        }

        public StringBuilder AppendLine(string value)
        {
            Append(value);
            return Append("\r\n");
        }

        public StringBuilder AppendFormat(string format, object arg0)
        {
            return AppendFormat(null, format, new object[] { arg0 });
        }

        public StringBuilder AppendFormat(string format, object arg0, object arg1)
        {
            return AppendFormat(null, format, new object[] { arg0, arg1 });
        }

        public StringBuilder AppendFormat(string format, object arg0, object arg1, object arg2)
        {
            return AppendFormat(null, format, new object[] { arg0, arg1, arg2 });
        }

        public StringBuilder AppendFormat(string format, params object[] args)
        {
            return AppendFormat(null, format, args);
        }

        // « {index[,alignement][:format]} », accolades doublées pour les
        // accolades littérales.
        public StringBuilder AppendFormat(IFormatProvider provider, string format, params object[] args)
        {
            if ((object)format == null)
                throw new ArgumentNullException("format");
            if (args == null)
                throw new ArgumentNullException("args");
            int i = 0;
            int n = format.Length;
            while (i < n)
            {
                char c = format[i];
                if (c == '}')
                {
                    if (i + 1 < n && format[i + 1] == '}')
                    {
                        Append('}');
                        i += 2;
                        continue;
                    }
                    throw BadFormat();
                }
                if (c != '{')
                {
                    Append(c);
                    i++;
                    continue;
                }
                if (i + 1 < n && format[i + 1] == '{')
                {
                    Append('{');
                    i += 2;
                    continue;
                }
                i++;
                int index = 0;
                int digits = 0;
                while (i < n && format[i] >= '0' && format[i] <= '9')
                {
                    index = index * 10 + (format[i] - '0');
                    digits++;
                    i++;
                }
                if (digits == 0)
                    throw BadFormat();
                while (i < n && format[i] == ' ') i++;
                int width = 0;
                bool left = false;
                if (i < n && format[i] == ',')
                {
                    i++;
                    while (i < n && format[i] == ' ') i++;
                    if (i < n && format[i] == '-')
                    {
                        left = true;
                        i++;
                    }
                    int wd = 0;
                    while (i < n && format[i] >= '0' && format[i] <= '9')
                    {
                        width = width * 10 + (format[i] - '0');
                        wd++;
                        i++;
                    }
                    if (wd == 0)
                        throw BadFormat();
                    while (i < n && format[i] == ' ') i++;
                }
                string itemFormat = null;
                if (i < n && format[i] == ':')
                {
                    i++;
                    int start = i;
                    while (i < n && format[i] != '}')
                    {
                        if (format[i] == '{')
                            throw BadFormat();
                        i++;
                    }
                    itemFormat = format.Substring(start, i - start);
                }
                if (i >= n || format[i] != '}')
                    throw BadFormat();
                i++;
                if (index >= args.Length)
                    throw new FormatException("Index (zero based) must be greater than or equal to zero and less than the size of the argument list.");
                string text = Number.FormatObject(args[index], itemFormat, provider);
                int pad = width - text.Length;
                if (!left && pad > 0)
                    Append(' ', pad);
                Append(text);
                if (left && pad > 0)
                    Append(' ', pad);
            }
            return this;
        }

        private static FormatException BadFormat()
        {
            return new FormatException("Input string was not in a correct format.");
        }

        public StringBuilder Insert(int index, string value)
        {
            if (index < 0 || index > _length)
                throw new ArgumentOutOfRangeException("index");
            if ((object)value == null || value.Length == 0)
                return this;
            int n = value.Length;
            EnsureCapacity(_length + n);
            for (int i = _length - 1; i >= index; i--)
                _chars[i + n] = _chars[i];
            for (int i = 0; i < n; i++)
                _chars[index + i] = value[i];
            _length += n;
            return this;
        }

        public StringBuilder Insert(int index, char value)
        {
            return Insert(index, new string(value, 1));
        }

        public StringBuilder Insert(int index, object value)
        {
            return value == null ? this : Insert(index, value.ToString());
        }

        public StringBuilder Insert(int index, int value)
        {
            return Insert(index, value.ToString());
        }

        public StringBuilder Remove(int startIndex, int length)
        {
            if (startIndex < 0 || length < 0 || startIndex + length > _length)
                throw new ArgumentOutOfRangeException("length");
            for (int i = startIndex + length; i < _length; i++)
                _chars[i - length] = _chars[i];
            _length -= length;
            return this;
        }

        public StringBuilder Replace(char oldChar, char newChar)
        {
            for (int i = 0; i < _length; i++)
            {
                if (_chars[i] == oldChar)
                    _chars[i] = newChar;
            }
            return this;
        }

        public StringBuilder Replace(string oldValue, string newValue)
        {
            if ((object)oldValue == null)
                throw new ArgumentNullException("oldValue");
            if (oldValue.Length == 0)
                throw new ArgumentException("String cannot be of zero length.");
            string text = ToString().Replace(oldValue, (object)newValue == null ? "" : newValue);
            _length = 0;
            return Append(text);
        }

        public override string ToString()
        {
            return new string(_chars, 0, _length);
        }

        public string ToString(int startIndex, int length)
        {
            if (startIndex < 0 || length < 0 || startIndex + length > _length)
                throw new ArgumentOutOfRangeException("length");
            return new string(_chars, startIndex, length);
        }
    }
}
