// BCL Winlux : primitives numériques (ToString, Parse, TryParse,
// CompareTo), Boolean, Char, IFormattable, NumberStyles et FormatException
// (#1956). Licence GPL 3. Écrit d'après la documentation publique de l'API
// (« Int32 Struct », « Double.ToString », « Standard numeric format
// strings », « NumberStyles Enum ») ; aucun code de Microsoft, de Mono ou de
// Wine.
//
// La mise en texte est servie par le moteur (runtime/src/clr/nombres.rs),
// avec les conventions de la culture invariante quel que soit le
// IFormatProvider donné.

using System.Collections.Generic;
using System.Globalization;
using System.Runtime.CompilerServices;

namespace System
{
    public interface IFormatProvider
    {
        object GetFormat(Type formatType);
    }

    public interface IFormattable
    {
        string ToString(string format, IFormatProvider formatProvider);
    }

    public class FormatException : SystemException
    {
        public FormatException() : base("One of the identified items was in an invalid format.") { }
        public FormatException(string message) : base(message) { }
    }

    public partial struct SByte : IComparable, IComparable<sbyte>, IEquatable<sbyte>, IFormattable
    {
        public const sbyte MaxValue = 127;
        public const sbyte MinValue = -128;

        public override string ToString()
        {
            return Number.FormatInteger((long)this, false, 1, null);
        }

        public string ToString(string format)
        {
            return Number.FormatInteger((long)this, false, 1, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        public int CompareTo(sbyte value)
        {
            return this < value ? -1 : (this > value ? 1 : 0);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is sbyte))
                throw new ArgumentException("Object must be of type SByte.");
            return CompareTo((sbyte)value);
        }

        public bool Equals(sbyte obj)
        {
            return this == obj;
        }

        public static sbyte Parse(string s)
        {
            return Parse(s, NumberStyles.Integer, null);
        }

        public static sbyte Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static sbyte Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Integer, provider);
        }

        public static sbyte Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            long r;
            Number.Check(Number.ParseInteger(s, style, false, 1, out r), "SByte");
            return (sbyte)r;
        }

        public static bool TryParse(string s, out sbyte result)
        {
            return TryParse(s, NumberStyles.Integer, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out sbyte result)
        {
            long r;
            bool ok = Number.ParseInteger(s, style, false, 1, out r) == 0;
            result = ok ? (sbyte)r : (sbyte)0;
            return ok;
        }
    }

    public partial struct Byte : IComparable, IComparable<byte>, IEquatable<byte>, IFormattable
    {
        public const byte MaxValue = 255;
        public const byte MinValue = 0;

        public override string ToString()
        {
            return Number.FormatInteger((long)this, true, 1, null);
        }

        public string ToString(string format)
        {
            return Number.FormatInteger((long)this, true, 1, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        public int CompareTo(byte value)
        {
            return this < value ? -1 : (this > value ? 1 : 0);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is byte))
                throw new ArgumentException("Object must be of type Byte.");
            return CompareTo((byte)value);
        }

        public bool Equals(byte obj)
        {
            return this == obj;
        }

        public static byte Parse(string s)
        {
            return Parse(s, NumberStyles.Integer, null);
        }

        public static byte Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static byte Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Integer, provider);
        }

        public static byte Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            long r;
            Number.Check(Number.ParseInteger(s, style, true, 1, out r), "Byte");
            return (byte)r;
        }

        public static bool TryParse(string s, out byte result)
        {
            return TryParse(s, NumberStyles.Integer, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out byte result)
        {
            long r;
            bool ok = Number.ParseInteger(s, style, true, 1, out r) == 0;
            result = ok ? (byte)r : (byte)0;
            return ok;
        }
    }

    public partial struct Int16 : IComparable, IComparable<short>, IEquatable<short>, IFormattable
    {
        public const short MaxValue = 32767;
        public const short MinValue = -32768;

        public override string ToString()
        {
            return Number.FormatInteger((long)this, false, 2, null);
        }

        public string ToString(string format)
        {
            return Number.FormatInteger((long)this, false, 2, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        public int CompareTo(short value)
        {
            return this < value ? -1 : (this > value ? 1 : 0);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is short))
                throw new ArgumentException("Object must be of type Int16.");
            return CompareTo((short)value);
        }

        public bool Equals(short obj)
        {
            return this == obj;
        }

        public static short Parse(string s)
        {
            return Parse(s, NumberStyles.Integer, null);
        }

        public static short Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static short Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Integer, provider);
        }

        public static short Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            long r;
            Number.Check(Number.ParseInteger(s, style, false, 2, out r), "Int16");
            return (short)r;
        }

        public static bool TryParse(string s, out short result)
        {
            return TryParse(s, NumberStyles.Integer, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out short result)
        {
            long r;
            bool ok = Number.ParseInteger(s, style, false, 2, out r) == 0;
            result = ok ? (short)r : (short)0;
            return ok;
        }
    }

    public partial struct UInt16 : IComparable, IComparable<ushort>, IEquatable<ushort>, IFormattable
    {
        public const ushort MaxValue = 65535;
        public const ushort MinValue = 0;

        public override string ToString()
        {
            return Number.FormatInteger((long)this, true, 2, null);
        }

        public string ToString(string format)
        {
            return Number.FormatInteger((long)this, true, 2, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        public int CompareTo(ushort value)
        {
            return this < value ? -1 : (this > value ? 1 : 0);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is ushort))
                throw new ArgumentException("Object must be of type UInt16.");
            return CompareTo((ushort)value);
        }

        public bool Equals(ushort obj)
        {
            return this == obj;
        }

        public static ushort Parse(string s)
        {
            return Parse(s, NumberStyles.Integer, null);
        }

        public static ushort Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static ushort Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Integer, provider);
        }

        public static ushort Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            long r;
            Number.Check(Number.ParseInteger(s, style, true, 2, out r), "UInt16");
            return (ushort)r;
        }

        public static bool TryParse(string s, out ushort result)
        {
            return TryParse(s, NumberStyles.Integer, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out ushort result)
        {
            long r;
            bool ok = Number.ParseInteger(s, style, true, 2, out r) == 0;
            result = ok ? (ushort)r : (ushort)0;
            return ok;
        }
    }

    public partial struct Int32 : IComparable, IComparable<int>, IEquatable<int>, IFormattable
    {
        public override string ToString()
        {
            return Number.FormatInteger((long)this, false, 4, null);
        }

        public string ToString(string format)
        {
            return Number.FormatInteger((long)this, false, 4, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        public int CompareTo(int value)
        {
            return this < value ? -1 : (this > value ? 1 : 0);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is int))
                throw new ArgumentException("Object must be of type Int32.");
            return CompareTo((int)value);
        }

        public bool Equals(int obj)
        {
            return this == obj;
        }

        public static int Parse(string s)
        {
            return Parse(s, NumberStyles.Integer, null);
        }

        public static int Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static int Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Integer, provider);
        }

        public static int Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            long r;
            Number.Check(Number.ParseInteger(s, style, false, 4, out r), "Int32");
            return (int)r;
        }

        public static bool TryParse(string s, out int result)
        {
            return TryParse(s, NumberStyles.Integer, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out int result)
        {
            long r;
            bool ok = Number.ParseInteger(s, style, false, 4, out r) == 0;
            result = ok ? (int)r : (int)0;
            return ok;
        }
    }

    public partial struct UInt32 : IComparable, IComparable<uint>, IEquatable<uint>, IFormattable
    {
        public const uint MaxValue = 0xFFFFFFFF;
        public const uint MinValue = 0;

        public override string ToString()
        {
            return Number.FormatInteger((long)this, true, 4, null);
        }

        public string ToString(string format)
        {
            return Number.FormatInteger((long)this, true, 4, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        public int CompareTo(uint value)
        {
            return this < value ? -1 : (this > value ? 1 : 0);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is uint))
                throw new ArgumentException("Object must be of type UInt32.");
            return CompareTo((uint)value);
        }

        public bool Equals(uint obj)
        {
            return this == obj;
        }

        public static uint Parse(string s)
        {
            return Parse(s, NumberStyles.Integer, null);
        }

        public static uint Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static uint Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Integer, provider);
        }

        public static uint Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            long r;
            Number.Check(Number.ParseInteger(s, style, true, 4, out r), "UInt32");
            return (uint)r;
        }

        public static bool TryParse(string s, out uint result)
        {
            return TryParse(s, NumberStyles.Integer, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out uint result)
        {
            long r;
            bool ok = Number.ParseInteger(s, style, true, 4, out r) == 0;
            result = ok ? (uint)r : (uint)0;
            return ok;
        }
    }

    public partial struct Int64 : IComparable, IComparable<long>, IEquatable<long>, IFormattable
    {
        public override string ToString()
        {
            return Number.FormatInteger((long)this, false, 8, null);
        }

        public string ToString(string format)
        {
            return Number.FormatInteger((long)this, false, 8, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        public int CompareTo(long value)
        {
            return this < value ? -1 : (this > value ? 1 : 0);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is long))
                throw new ArgumentException("Object must be of type Int64.");
            return CompareTo((long)value);
        }

        public bool Equals(long obj)
        {
            return this == obj;
        }

        public static long Parse(string s)
        {
            return Parse(s, NumberStyles.Integer, null);
        }

        public static long Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static long Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Integer, provider);
        }

        public static long Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            long r;
            Number.Check(Number.ParseInteger(s, style, false, 8, out r), "Int64");
            return (long)r;
        }

        public static bool TryParse(string s, out long result)
        {
            return TryParse(s, NumberStyles.Integer, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out long result)
        {
            long r;
            bool ok = Number.ParseInteger(s, style, false, 8, out r) == 0;
            result = ok ? (long)r : (long)0;
            return ok;
        }
    }

    public partial struct UInt64 : IComparable, IComparable<ulong>, IEquatable<ulong>, IFormattable
    {
        public const ulong MaxValue = 0xFFFFFFFFFFFFFFFF;
        public const ulong MinValue = 0;

        public override string ToString()
        {
            return Number.FormatInteger((long)this, true, 8, null);
        }

        public string ToString(string format)
        {
            return Number.FormatInteger((long)this, true, 8, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        public int CompareTo(ulong value)
        {
            return this < value ? -1 : (this > value ? 1 : 0);
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is ulong))
                throw new ArgumentException("Object must be of type UInt64.");
            return CompareTo((ulong)value);
        }

        public bool Equals(ulong obj)
        {
            return this == obj;
        }

        public static ulong Parse(string s)
        {
            return Parse(s, NumberStyles.Integer, null);
        }

        public static ulong Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static ulong Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Integer, provider);
        }

        public static ulong Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            long r;
            Number.Check(Number.ParseInteger(s, style, true, 8, out r), "UInt64");
            return (ulong)r;
        }

        public static bool TryParse(string s, out ulong result)
        {
            return TryParse(s, NumberStyles.Integer, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out ulong result)
        {
            long r;
            bool ok = Number.ParseInteger(s, style, true, 8, out r) == 0;
            result = ok ? (ulong)r : (ulong)0;
            return ok;
        }
    }

    public partial struct Double : IComparable, IComparable<double>, IEquatable<double>, IFormattable
    {
        public const double MaxValue = 1.7976931348623157E+308;
        public const double MinValue = -1.7976931348623157E+308;
        public const double Epsilon = 4.94065645841247E-324;
        public const double NaN = 0.0 / 0.0;
        public const double PositiveInfinity = 1.0 / 0.0;
        public const double NegativeInfinity = -1.0 / 0.0;

        public static bool IsInfinity(double d) { return d == PositiveInfinity || d == NegativeInfinity; }
        public static bool IsPositiveInfinity(double d) { return d == PositiveInfinity; }
        public static bool IsNegativeInfinity(double d) { return d == NegativeInfinity; }

        public override string ToString()
        {
            return Number.FormatDouble(this, false, null);
        }

        public string ToString(string format)
        {
            return Number.FormatDouble(this, false, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        // NaN passe avant tout nombre et égale NaN (Double.CompareTo).
        public int CompareTo(double value)
        {
            if (this < value) return -1;
            if (this > value) return 1;
            if (this == value) return 0;
            if (IsNaN(this)) return IsNaN(value) ? 0 : -1;
            return 1;
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is double))
                throw new ArgumentException("Object must be of type Double.");
            return CompareTo((double)value);
        }

        public bool Equals(double obj)
        {
            return this == obj || (IsNaN(this) && IsNaN(obj));
        }

        public static double Parse(string s)
        {
            return Parse(s, NumberStyles.Float | NumberStyles.AllowThousands, null);
        }

        public static double Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Float | NumberStyles.AllowThousands, provider);
        }

        public static double Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static double Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            if ((object)s == null)
                throw new ArgumentNullException("s");
            double r;
            if (!Number.TryParseDouble(s, out r))
                throw new FormatException("Input string was not in a correct format.");
            return (double)r;
        }

        public static bool TryParse(string s, out double result)
        {
            return TryParse(s, NumberStyles.Float | NumberStyles.AllowThousands, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out double result)
        {
            double r = 0;
            bool ok = (object)s != null && Number.TryParseDouble(s, out r);
            result = ok ? (double)r : (double)0;
            return ok;
        }
    }

    public partial struct Single : IComparable, IComparable<float>, IEquatable<float>, IFormattable
    {
        public const float MaxValue = 3.40282347E+38f;
        public const float MinValue = -3.40282347E+38f;
        public const float Epsilon = 1.401298E-45f;
        public const float NaN = 0.0f / 0.0f;
        public const float PositiveInfinity = 1.0f / 0.0f;
        public const float NegativeInfinity = -1.0f / 0.0f;

#pragma warning disable 1718
        public static bool IsNaN(float f) { return f != f; }
#pragma warning restore 1718

        public static bool IsInfinity(float d) { return d == PositiveInfinity || d == NegativeInfinity; }
        public static bool IsPositiveInfinity(float d) { return d == PositiveInfinity; }
        public static bool IsNegativeInfinity(float d) { return d == NegativeInfinity; }

        public override string ToString()
        {
            return Number.FormatDouble(this, true, null);
        }

        public string ToString(string format)
        {
            return Number.FormatDouble(this, true, format);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public string ToString(string format, IFormatProvider provider)
        {
            return ToString(format);
        }

        // NaN passe avant tout nombre et égale NaN (Double.CompareTo).
        public int CompareTo(float value)
        {
            if (this < value) return -1;
            if (this > value) return 1;
            if (this == value) return 0;
            if (IsNaN(this)) return IsNaN(value) ? 0 : -1;
            return 1;
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is float))
                throw new ArgumentException("Object must be of type Single.");
            return CompareTo((float)value);
        }

        public bool Equals(float obj)
        {
            return this == obj || (IsNaN(this) && IsNaN(obj));
        }

        public static float Parse(string s)
        {
            return Parse(s, NumberStyles.Float | NumberStyles.AllowThousands, null);
        }

        public static float Parse(string s, IFormatProvider provider)
        {
            return Parse(s, NumberStyles.Float | NumberStyles.AllowThousands, provider);
        }

        public static float Parse(string s, NumberStyles style)
        {
            return Parse(s, style, null);
        }

        public static float Parse(string s, NumberStyles style, IFormatProvider provider)
        {
            if ((object)s == null)
                throw new ArgumentNullException("s");
            double r;
            if (!Number.TryParseDouble(s, out r))
                throw new FormatException("Input string was not in a correct format.");
            return (float)r;
        }

        public static bool TryParse(string s, out float result)
        {
            return TryParse(s, NumberStyles.Float | NumberStyles.AllowThousands, null, out result);
        }

        public static bool TryParse(string s, NumberStyles style, IFormatProvider provider, out float result)
        {
            double r = 0;
            bool ok = (object)s != null && Number.TryParseDouble(s, out r);
            result = ok ? (float)r : (float)0;
            return ok;
        }
    }

    public partial struct Boolean : IComparable, IComparable<bool>, IEquatable<bool>
    {
        public static readonly string TrueString = "True";
        public static readonly string FalseString = "False";

        public override string ToString()
        {
            return this ? "True" : "False";
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public int CompareTo(bool value)
        {
            return this == value ? 0 : (this ? 1 : -1);
        }

        public int CompareTo(object obj)
        {
            if (obj == null)
                return 1;
            if (!(obj is bool))
                throw new ArgumentException("Object must be of type Boolean.");
            return CompareTo((bool)obj);
        }

        public bool Equals(bool obj)
        {
            return this == obj;
        }

        public static bool Parse(string value)
        {
            if ((object)value == null)
                throw new ArgumentNullException("value");
            bool result;
            if (!TryParse(value, out result))
                throw new FormatException("String was not recognized as a valid Boolean.");
            return result;
        }

        public static bool TryParse(string value, out bool result)
        {
            result = false;
            if ((object)value == null)
                return false;
            string t = value.Trim();
            if (string.Equals(t, "True", StringComparison.OrdinalIgnoreCase))
            {
                result = true;
                return true;
            }
            return string.Equals(t, "False", StringComparison.OrdinalIgnoreCase);
        }
    }

    public partial struct Char : IComparable, IComparable<char>, IEquatable<char>
    {
        public const char MaxValue = (char)0xFFFF;
        public const char MinValue = (char)0;

        public override string ToString()
        {
            return new string(this, 1);
        }

        public string ToString(IFormatProvider provider)
        {
            return ToString();
        }

        public static string ToString(char c)
        {
            return new string(c, 1);
        }

        public int CompareTo(char value)
        {
            return this - value;
        }

        public int CompareTo(object value)
        {
            if (value == null)
                return 1;
            if (!(value is char))
                throw new ArgumentException("Object must be of type Char.");
            return this - (char)value;
        }

        public bool Equals(char obj)
        {
            return this == obj;
        }

        public static char Parse(string s)
        {
            if ((object)s == null)
                throw new ArgumentNullException("s");
            if (s.Length != 1)
                throw new FormatException("String must be exactly one character long.");
            return s[0];
        }

        public static bool TryParse(string s, out char result)
        {
            bool ok = (object)s != null && s.Length == 1;
            result = ok ? s[0] : '\0';
            return ok;
        }

        // Catégories Unicode servies par le moteur (bcl.rs).
        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern char ToUpper(char c);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern char ToLower(char c);

        public static char ToUpperInvariant(char c) { return ToUpper(c); }
        public static char ToLowerInvariant(char c) { return ToLower(c); }

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern bool IsLetter(char c);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern bool IsUpper(char c);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern bool IsLower(char c);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern bool IsNumber(char c);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern bool IsPunctuation(char c);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern bool IsControl(char c);

        public static bool IsLetterOrDigit(char c) { return IsLetter(c) || IsDigit(c); }
        public static bool IsSurrogate(char c) { return c >= 0xD800 && c <= 0xDFFF; }
        public static bool IsHighSurrogate(char c) { return c >= 0xD800 && c <= 0xDBFF; }
        public static bool IsLowSurrogate(char c) { return c >= 0xDC00 && c <= 0xDFFF; }

        public static bool IsWhiteSpace(string s, int index) { return IsWhiteSpace(s[index]); }
        public static bool IsDigit(string s, int index) { return IsDigit(s[index]); }
        public static bool IsLetter(string s, int index) { return IsLetter(s[index]); }
        public static bool IsLetterOrDigit(string s, int index) { return IsLetterOrDigit(s[index]); }
        public static bool IsUpper(string s, int index) { return IsUpper(s[index]); }
        public static bool IsLower(string s, int index) { return IsLower(s[index]); }
    }

    internal static class Number
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string FormatInteger(long value, bool isUnsigned, int size, string format);

        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string FormatDouble(double value, bool isSingle, string format);

        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern bool TryParseDouble(string s, out double result);

        private static bool IsSpace(char c)
        {
            return c == ' ' || (c >= '\t' && c <= '\r');
        }

        // 0 : lu ; 1 : format invalide ; 2 : hors limites ; 3 : null.
        internal static int ParseInteger(string s, NumberStyles style, bool unsigned, int size, out long result)
        {
            result = 0;
            if ((object)s == null)
                return 3;
            int i = 0;
            int n = s.Length;
            if ((style & NumberStyles.AllowLeadingWhite) != 0)
                while (i < n && IsSpace(s[i])) i++;
            if ((style & NumberStyles.AllowTrailingWhite) != 0)
                while (n > i && IsSpace(s[n - 1])) n--;
            bool hex = (style & NumberStyles.AllowHexSpecifier) != 0;
            bool neg = false;
            if (!hex && (style & NumberStyles.AllowLeadingSign) != 0 && i < n && (s[i] == '-' || s[i] == '+'))
            {
                neg = s[i] == '-';
                i++;
            }
            else if (!hex && (style & NumberStyles.AllowParentheses) != 0 && n - i >= 2 && s[i] == '(' && s[n - 1] == ')')
            {
                neg = true;
                i++;
                n--;
            }
            else if (!hex && (style & NumberStyles.AllowTrailingSign) != 0 && n > i && (s[n - 1] == '-' || s[n - 1] == '+'))
            {
                neg = s[n - 1] == '-';
                n--;
            }
            ulong radix = hex ? 16UL : 10UL;
            ulong mag = 0;
            int digits = 0;
            bool overflow = false;
            bool fraction = false;
            for (; i < n; i++)
            {
                char c = s[i];
                int d;
                if (c >= '0' && c <= '9')
                    d = c - '0';
                else if (hex && c >= 'a' && c <= 'f')
                    d = c - 'a' + 10;
                else if (hex && c >= 'A' && c <= 'F')
                    d = c - 'A' + 10;
                else if (!hex && c == ',' && digits > 0 && !fraction && (style & NumberStyles.AllowThousands) != 0)
                    continue;
                else if (!hex && c == '.' && !fraction && (style & NumberStyles.AllowDecimalPoint) != 0)
                {
                    fraction = true;
                    continue;
                }
                else
                    return 1;
                if (fraction)
                {
                    // Une partie décimale non nulle ne tient pas dans un entier.
                    if (d != 0)
                        overflow = true;
                    continue;
                }
                digits++;
                if (mag > (ulong.MaxValue - (ulong)d) / radix)
                    overflow = true;
                else
                    mag = mag * radix + (ulong)d;
            }
            if (digits == 0)
                return 1;
            if (overflow)
                return 2;
            int bits = size * 8;
            ulong umax = bits == 64 ? ulong.MaxValue : (1UL << bits) - 1;
            if (hex)
            {
                if (mag > umax)
                    return 2;
                if (!unsigned && bits < 64 && (mag & (1UL << (bits - 1))) != 0)
                    mag |= ~umax;
                result = (long)mag;
                return 0;
            }
            if (unsigned)
            {
                if ((neg && mag != 0) || mag > umax)
                    return 2;
                result = (long)mag;
                return 0;
            }
            ulong limit = 1UL << (bits - 1);
            if (neg)
            {
                if (mag > limit)
                    return 2;
                result = (long)(0UL - mag);
                return 0;
            }
            if (mag >= limit)
                return 2;
            result = (long)mag;
            return 0;
        }

        internal static void Check(int code, string typeName)
        {
            if (code == 1)
                throw new FormatException("Input string was not in a correct format.");
            if (code == 2)
                throw new OverflowException(string.Concat("Value was either too large or too small for ", typeName == "Int32" || typeName == "Int64" || typeName == "Int16" ? "an " : "a ", typeName, "."));
            if (code == 3)
                throw new ArgumentNullException("s");
        }

        // Texte d'un argument de String.Format : les primitives et les
        // IFormattable reçoivent le format, les autres leur ToString().
        internal static string FormatObject(object arg, string format, IFormatProvider provider)
        {
            if (arg == null)
                return "";
            if ((object)format != null && format.Length > 0)
            {
                if (arg is int) return ((int)arg).ToString(format);
                if (arg is long) return ((long)arg).ToString(format);
                if (arg is uint) return ((uint)arg).ToString(format);
                if (arg is ulong) return ((ulong)arg).ToString(format);
                if (arg is short) return ((short)arg).ToString(format);
                if (arg is ushort) return ((ushort)arg).ToString(format);
                if (arg is byte) return ((byte)arg).ToString(format);
                if (arg is sbyte) return ((sbyte)arg).ToString(format);
                if (arg is double) return ((double)arg).ToString(format);
                if (arg is float) return ((float)arg).ToString(format);
                IFormattable f = arg as IFormattable;
                if (f != null)
                    return f.ToString(format, provider);
            }
            string s = arg.ToString();
            return (object)s == null ? "" : s;
        }
    }
}

namespace System.Globalization
{
    [Flags]
    public enum NumberStyles
    {
        None = 0,
        AllowLeadingWhite = 1,
        AllowTrailingWhite = 2,
        AllowLeadingSign = 4,
        AllowTrailingSign = 8,
        AllowParentheses = 16,
        AllowDecimalPoint = 32,
        AllowThousands = 64,
        AllowExponent = 128,
        AllowCurrencySymbol = 256,
        AllowHexSpecifier = 512,
        Integer = 7,
        HexNumber = 515,
        Number = 111,
        Float = 167,
        Currency = 383,
        Any = 511,
    }
}
