// BCL Winlux : System.Convert (#1962). Licence GPL 3.
// Écrit d'après la documentation publique de l'API (« Convert Class ») ;
// aucun code de Microsoft, de Mono ou de Wine.
//
// Conversions vérifiées : OverflowException hors limites, arrondi au pair
// le plus proche depuis un flottant, comme le documente Convert.ToInt32.
// Les chaînes se lisent avec les Parse des primitives (culture invariante).

namespace System
{
    public interface IConvertible
    {
        bool ToBoolean(IFormatProvider provider);
        char ToChar(IFormatProvider provider);
        sbyte ToSByte(IFormatProvider provider);
        byte ToByte(IFormatProvider provider);
        short ToInt16(IFormatProvider provider);
        ushort ToUInt16(IFormatProvider provider);
        int ToInt32(IFormatProvider provider);
        uint ToUInt32(IFormatProvider provider);
        long ToInt64(IFormatProvider provider);
        ulong ToUInt64(IFormatProvider provider);
        float ToSingle(IFormatProvider provider);
        double ToDouble(IFormatProvider provider);
        string ToString(IFormatProvider provider);
    }

    public class InvalidCastException : SystemException
    {
        public InvalidCastException() : base("Specified cast is not valid.") { }
        public InvalidCastException(string message) : base(message) { }
    }

    // Array.Copy entre tableaux de structures différentes (#2037).
    public class ArrayTypeMismatchException : SystemException
    {
        public ArrayTypeMismatchException() : base("Attempted to access an element as a type incompatible with the array.") { }
        public ArrayTypeMismatchException(string message) : base(message) { }
    }

    public static class Convert
    {
        private static OverflowException Overflow(string typeName)
        {
            return new OverflowException(string.Concat("Value was either too large or too small for ",
                typeName == "Int16" || typeName == "Int32" || typeName == "Int64" || typeName == "UInt16"
                    || typeName == "UInt32" || typeName == "UInt64" ? "an " : "a ", typeName, "."));
        }

        // Arrondi au pair le plus proche (« banker's rounding »).
        private static double Round(double value)
        {
            double t = value < 0 ? -(double)(long)-value : (double)(long)value;
            if (value > 9.2e18 || value < -9.2e18)
                return value;
            double frac = value - t;
            if (frac > 0.5 || (frac == 0.5 && ((long)t & 1) != 0))
                return t + 1;
            if (frac < -0.5 || (frac == -0.5 && ((long)t & 1) != 0))
                return t - 1;
            return t;
        }

        private static int Digit(char c)
        {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            if (c >= 'A' && c <= 'F') return c - 'A' + 10;
            return 99;
        }

        // Base 2, 8, 10 ou 16 ; en base 16, préfixe « 0x » permis. Hors base
        // 10, le texte est le complément à deux du type (« FF » pour -1 en
        // sbyte).
        private static long ParseBase(string value, int fromBase, int size, bool unsigned)
        {
            if (fromBase != 2 && fromBase != 8 && fromBase != 10 && fromBase != 16)
                throw new ArgumentException("Invalid Base.");
            if ((object)value == null)
                return 0;
            if (fromBase == 10)
            {
                long r10;
                Number.Check(Number.ParseInteger(value, System.Globalization.NumberStyles.Integer, unsigned, size, out r10),
                    size == 1 ? (unsigned ? "Byte" : "SByte") : size == 2 ? (unsigned ? "UInt16" : "Int16")
                    : size == 4 ? (unsigned ? "UInt32" : "Int32") : (unsigned ? "UInt64" : "Int64"));
                return r10;
            }
            int i = 0;
            bool neg = false;
            if (value.Length > 0 && value[0] == '-')
            {
                neg = true;
                i = 1;
            }
            if (fromBase == 16 && value.Length - i >= 2 && value[i] == '0' && (value[i + 1] == 'x' || value[i + 1] == 'X'))
                i += 2;
            if (i >= value.Length)
                throw new FormatException("Could not find any recognizable digits.");
            ulong mag = 0;
            int bits = size * 8;
            ulong umax = bits == 64 ? ulong.MaxValue : (1UL << bits) - 1;
            for (; i < value.Length; i++)
            {
                int d = Digit(value[i]);
                if (d >= fromBase)
                    throw new FormatException("Could not find any recognizable digits.");
                if (mag > (umax - (ulong)d) / (ulong)fromBase)
                    throw new OverflowException("Value was either too large or too small.");
                mag = mag * (ulong)fromBase + (ulong)d;
            }
            if (neg)
            {
                if (fromBase != 10)
                    throw new ArgumentException("String cannot contain a minus sign if the base is not 10.");
            }
            if (!unsigned && bits < 64 && (mag & (1UL << (bits - 1))) != 0)
                mag |= ~umax;
            return (long)mag;
        }

        private static string ToBase(ulong value, int toBase, int size, bool negative)
        {
            if (toBase != 2 && toBase != 8 && toBase != 10 && toBase != 16)
                throw new ArgumentException("Invalid Base.");
            int bits = size * 8;
            ulong mask = bits == 64 ? ulong.MaxValue : (1UL << bits) - 1;
            if (toBase == 10)
                return negative ? string.Concat("-", (0UL - value).ToString()) : value.ToString();
            ulong v = value & mask;
            if (v == 0)
                return "0";
            const string digits = "0123456789abcdef";
            char[] buf = new char[64];
            int pos = 64;
            while (v != 0)
            {
                buf[--pos] = digits[(int)(v % (ulong)toBase)];
                v /= (ulong)toBase;
            }
            return new string(buf, pos, 64 - pos);
        }

        private static readonly string Base64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        public static string ToBase64String(byte[] inArray)
        {
            if (inArray == null)
                throw new ArgumentNullException("inArray");
            return ToBase64String(inArray, 0, inArray.Length);
        }

        public static string ToBase64String(byte[] inArray, int offset, int length)
        {
            if (inArray == null)
                throw new ArgumentNullException("inArray");
            if (offset < 0 || length < 0 || offset > inArray.Length - length)
                throw new ArgumentOutOfRangeException("offset");
            char[] chars = new char[(length + 2) / 3 * 4];
            int o = 0;
            for (int i = 0; i < length; i += 3)
            {
                int b0 = inArray[offset + i];
                int b1 = i + 1 < length ? inArray[offset + i + 1] : 0;
                int b2 = i + 2 < length ? inArray[offset + i + 2] : 0;
                chars[o++] = Base64[b0 >> 2];
                chars[o++] = Base64[((b0 & 3) << 4) | (b1 >> 4)];
                chars[o++] = i + 1 < length ? Base64[((b1 & 15) << 2) | (b2 >> 6)] : '=';
                chars[o++] = i + 2 < length ? Base64[b2 & 63] : '=';
            }
            return new string(chars);
        }

        public static byte[] FromBase64String(string s)
        {
            if ((object)s == null)
                throw new ArgumentNullException("s");
            int[] q = new int[s.Length];
            int n = 0;
            int pad = 0;
            for (int i = 0; i < s.Length; i++)
            {
                char c = s[i];
                if (c == ' ' || c == '\t' || c == '\r' || c == '\n')
                    continue;
                if (c == '=')
                {
                    pad++;
                    continue;
                }
                if (pad > 0)
                    throw BadBase64();
                int v = Base64.IndexOf(c);
                if (v < 0)
                    throw BadBase64();
                q[n++] = v;
            }
            if ((n + pad) % 4 != 0 || pad > 2)
                throw BadBase64();
            byte[] result = new byte[n * 3 / 4];
            int o = 0;
            for (int i = 0; i + 1 < n; i += 4)
            {
                int v = (q[i] << 18) | (q[i + 1] << 12) | (i + 2 < n ? q[i + 2] << 6 : 0) | (i + 3 < n ? q[i + 3] : 0);
                result[o++] = (byte)(v >> 16);
                if (i + 2 < n) result[o++] = (byte)(v >> 8);
                if (i + 3 < n) result[o++] = (byte)v;
            }
            return result;
        }

        private static FormatException BadBase64()
        {
            return new FormatException("The input is not a valid Base-64 string as it contains a non-base 64 character, more than two padding characters, or an illegal character among the padding characters.");
        }

        public static sbyte ToSByte(object value) { return value == null ? (sbyte)0 : ToSByte(value, null); }
        public static sbyte ToSByte(object value, IFormatProvider provider)
        {
            if (value == null) return (sbyte)0;
            string s = value as string;
            if ((object)s != null) return sbyte.Parse(s, provider);
            if (value is bool) return ToSByte((bool)value);
            if (value is sbyte) return ToSByte((sbyte)value);
            if (value is byte) return ToSByte((byte)value);
            if (value is short) return ToSByte((short)value);
            if (value is ushort) return ToSByte((ushort)value);
            if (value is int) return ToSByte((int)value);
            if (value is uint) return ToSByte((uint)value);
            if (value is long) return ToSByte((long)value);
            if (value is ulong) return ToSByte((ulong)value);
            if (value is char) return ToSByte((char)value);
            if (value is double) return ToSByte((double)value);
            if (value is float) return ToSByte((float)value);
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToSByte(provider);
            throw new InvalidCastException("Unable to cast object to SByte.");
        }
        public static sbyte ToSByte(string value) { return (object)value == null ? (sbyte)0 : sbyte.Parse(value); }
        public static sbyte ToSByte(string value, IFormatProvider provider) { return (object)value == null ? (sbyte)0 : sbyte.Parse(value, provider); }
        public static sbyte ToSByte(string value, int fromBase) { return (sbyte)ParseBase(value, fromBase, 1, false); }
        public static sbyte ToSByte(bool value) { return value ? (sbyte)1 : (sbyte)0; }
        public static sbyte ToSByte(sbyte value) { return value; }
        public static sbyte ToSByte(byte value) { if (value > 127) throw Overflow("SByte"); return (sbyte)value; }
        public static sbyte ToSByte(short value) { if (value < -128 || value > 127) throw Overflow("SByte"); return (sbyte)value; }
        public static sbyte ToSByte(ushort value) { if (value > 127) throw Overflow("SByte"); return (sbyte)value; }
        public static sbyte ToSByte(int value) { if (value < -128 || value > 127) throw Overflow("SByte"); return (sbyte)value; }
        public static sbyte ToSByte(uint value) { if (value > 127) throw Overflow("SByte"); return (sbyte)value; }
        public static sbyte ToSByte(long value) { if (value < -128 || value > 127) throw Overflow("SByte"); return (sbyte)value; }
        public static sbyte ToSByte(ulong value) { if (value > 127) throw Overflow("SByte"); return (sbyte)value; }
        public static sbyte ToSByte(char value) { if (value > 127) throw Overflow("SByte"); return (sbyte)value; }
        public static sbyte ToSByte(double value)
        {
            if (!(value > -128.0 - 1.0 && value < 127.0 + 1.0)) throw Overflow("SByte");
            double r = Round(value);
            if (r < -128.0 || r > 127.0) throw Overflow("SByte");
            return (sbyte)r;
        }
        public static sbyte ToSByte(float value) { return ToSByte((double)value); }

        public static byte ToByte(object value) { return value == null ? (byte)0 : ToByte(value, null); }
        public static byte ToByte(object value, IFormatProvider provider)
        {
            if (value == null) return (byte)0;
            string s = value as string;
            if ((object)s != null) return byte.Parse(s, provider);
            if (value is bool) return ToByte((bool)value);
            if (value is sbyte) return ToByte((sbyte)value);
            if (value is byte) return ToByte((byte)value);
            if (value is short) return ToByte((short)value);
            if (value is ushort) return ToByte((ushort)value);
            if (value is int) return ToByte((int)value);
            if (value is uint) return ToByte((uint)value);
            if (value is long) return ToByte((long)value);
            if (value is ulong) return ToByte((ulong)value);
            if (value is char) return ToByte((char)value);
            if (value is double) return ToByte((double)value);
            if (value is float) return ToByte((float)value);
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToByte(provider);
            throw new InvalidCastException("Unable to cast object to Byte.");
        }
        public static byte ToByte(string value) { return (object)value == null ? (byte)0 : byte.Parse(value); }
        public static byte ToByte(string value, IFormatProvider provider) { return (object)value == null ? (byte)0 : byte.Parse(value, provider); }
        public static byte ToByte(string value, int fromBase) { return (byte)ParseBase(value, fromBase, 1, true); }
        public static byte ToByte(bool value) { return value ? (byte)1 : (byte)0; }
        public static byte ToByte(sbyte value) { if (value < 0) throw Overflow("Byte"); return (byte)value; }
        public static byte ToByte(byte value) { return value; }
        public static byte ToByte(short value) { if (value < 0 || value > 255) throw Overflow("Byte"); return (byte)value; }
        public static byte ToByte(ushort value) { if (value > 255) throw Overflow("Byte"); return (byte)value; }
        public static byte ToByte(int value) { if (value < 0 || value > 255) throw Overflow("Byte"); return (byte)value; }
        public static byte ToByte(uint value) { if (value > 255) throw Overflow("Byte"); return (byte)value; }
        public static byte ToByte(long value) { if (value < 0 || value > 255) throw Overflow("Byte"); return (byte)value; }
        public static byte ToByte(ulong value) { if (value > 255) throw Overflow("Byte"); return (byte)value; }
        public static byte ToByte(char value) { if (value > 255) throw Overflow("Byte"); return (byte)value; }
        public static byte ToByte(double value)
        {
            if (!(value > 0.0 - 1.0 && value < 255.0 + 1.0)) throw Overflow("Byte");
            double r = Round(value);
            if (r < 0.0 || r > 255.0) throw Overflow("Byte");
            return (byte)r;
        }
        public static byte ToByte(float value) { return ToByte((double)value); }

        public static short ToInt16(object value) { return value == null ? (short)0 : ToInt16(value, null); }
        public static short ToInt16(object value, IFormatProvider provider)
        {
            if (value == null) return (short)0;
            string s = value as string;
            if ((object)s != null) return short.Parse(s, provider);
            if (value is bool) return ToInt16((bool)value);
            if (value is sbyte) return ToInt16((sbyte)value);
            if (value is byte) return ToInt16((byte)value);
            if (value is short) return ToInt16((short)value);
            if (value is ushort) return ToInt16((ushort)value);
            if (value is int) return ToInt16((int)value);
            if (value is uint) return ToInt16((uint)value);
            if (value is long) return ToInt16((long)value);
            if (value is ulong) return ToInt16((ulong)value);
            if (value is char) return ToInt16((char)value);
            if (value is double) return ToInt16((double)value);
            if (value is float) return ToInt16((float)value);
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToInt16(provider);
            throw new InvalidCastException("Unable to cast object to Int16.");
        }
        public static short ToInt16(string value) { return (object)value == null ? (short)0 : short.Parse(value); }
        public static short ToInt16(string value, IFormatProvider provider) { return (object)value == null ? (short)0 : short.Parse(value, provider); }
        public static short ToInt16(string value, int fromBase) { return (short)ParseBase(value, fromBase, 2, false); }
        public static short ToInt16(bool value) { return value ? (short)1 : (short)0; }
        public static short ToInt16(sbyte value) { return (short)value; }
        public static short ToInt16(byte value) { return (short)value; }
        public static short ToInt16(short value) { return value; }
        public static short ToInt16(ushort value) { if (value > 32767) throw Overflow("Int16"); return (short)value; }
        public static short ToInt16(int value) { if (value < -32768 || value > 32767) throw Overflow("Int16"); return (short)value; }
        public static short ToInt16(uint value) { if (value > 32767) throw Overflow("Int16"); return (short)value; }
        public static short ToInt16(long value) { if (value < -32768 || value > 32767) throw Overflow("Int16"); return (short)value; }
        public static short ToInt16(ulong value) { if (value > 32767) throw Overflow("Int16"); return (short)value; }
        public static short ToInt16(char value) { if (value > 32767) throw Overflow("Int16"); return (short)value; }
        public static short ToInt16(double value)
        {
            if (!(value > -32768.0 - 1.0 && value < 32767.0 + 1.0)) throw Overflow("Int16");
            double r = Round(value);
            if (r < -32768.0 || r > 32767.0) throw Overflow("Int16");
            return (short)r;
        }
        public static short ToInt16(float value) { return ToInt16((double)value); }

        public static ushort ToUInt16(object value) { return value == null ? (ushort)0 : ToUInt16(value, null); }
        public static ushort ToUInt16(object value, IFormatProvider provider)
        {
            if (value == null) return (ushort)0;
            string s = value as string;
            if ((object)s != null) return ushort.Parse(s, provider);
            if (value is bool) return ToUInt16((bool)value);
            if (value is sbyte) return ToUInt16((sbyte)value);
            if (value is byte) return ToUInt16((byte)value);
            if (value is short) return ToUInt16((short)value);
            if (value is ushort) return ToUInt16((ushort)value);
            if (value is int) return ToUInt16((int)value);
            if (value is uint) return ToUInt16((uint)value);
            if (value is long) return ToUInt16((long)value);
            if (value is ulong) return ToUInt16((ulong)value);
            if (value is char) return ToUInt16((char)value);
            if (value is double) return ToUInt16((double)value);
            if (value is float) return ToUInt16((float)value);
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToUInt16(provider);
            throw new InvalidCastException("Unable to cast object to UInt16.");
        }
        public static ushort ToUInt16(string value) { return (object)value == null ? (ushort)0 : ushort.Parse(value); }
        public static ushort ToUInt16(string value, IFormatProvider provider) { return (object)value == null ? (ushort)0 : ushort.Parse(value, provider); }
        public static ushort ToUInt16(string value, int fromBase) { return (ushort)ParseBase(value, fromBase, 2, true); }
        public static ushort ToUInt16(bool value) { return value ? (ushort)1 : (ushort)0; }
        public static ushort ToUInt16(sbyte value) { if (value < 0) throw Overflow("UInt16"); return (ushort)value; }
        public static ushort ToUInt16(byte value) { return (ushort)value; }
        public static ushort ToUInt16(short value) { if (value < 0) throw Overflow("UInt16"); return (ushort)value; }
        public static ushort ToUInt16(ushort value) { return value; }
        public static ushort ToUInt16(int value) { if (value < 0 || value > 65535) throw Overflow("UInt16"); return (ushort)value; }
        public static ushort ToUInt16(uint value) { if (value > 65535) throw Overflow("UInt16"); return (ushort)value; }
        public static ushort ToUInt16(long value) { if (value < 0 || value > 65535) throw Overflow("UInt16"); return (ushort)value; }
        public static ushort ToUInt16(ulong value) { if (value > 65535) throw Overflow("UInt16"); return (ushort)value; }
        public static ushort ToUInt16(char value) { return (ushort)value; }
        public static ushort ToUInt16(double value)
        {
            if (!(value > 0.0 - 1.0 && value < 65535.0 + 1.0)) throw Overflow("UInt16");
            double r = Round(value);
            if (r < 0.0 || r > 65535.0) throw Overflow("UInt16");
            return (ushort)r;
        }
        public static ushort ToUInt16(float value) { return ToUInt16((double)value); }

        public static int ToInt32(object value) { return value == null ? (int)0 : ToInt32(value, null); }
        public static int ToInt32(object value, IFormatProvider provider)
        {
            if (value == null) return (int)0;
            string s = value as string;
            if ((object)s != null) return int.Parse(s, provider);
            if (value is bool) return ToInt32((bool)value);
            if (value is sbyte) return ToInt32((sbyte)value);
            if (value is byte) return ToInt32((byte)value);
            if (value is short) return ToInt32((short)value);
            if (value is ushort) return ToInt32((ushort)value);
            if (value is int) return ToInt32((int)value);
            if (value is uint) return ToInt32((uint)value);
            if (value is long) return ToInt32((long)value);
            if (value is ulong) return ToInt32((ulong)value);
            if (value is char) return ToInt32((char)value);
            if (value is double) return ToInt32((double)value);
            if (value is float) return ToInt32((float)value);
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToInt32(provider);
            throw new InvalidCastException("Unable to cast object to Int32.");
        }
        public static int ToInt32(string value) { return (object)value == null ? (int)0 : int.Parse(value); }
        public static int ToInt32(string value, IFormatProvider provider) { return (object)value == null ? (int)0 : int.Parse(value, provider); }
        public static int ToInt32(string value, int fromBase) { return (int)ParseBase(value, fromBase, 4, false); }
        public static int ToInt32(bool value) { return value ? (int)1 : (int)0; }
        public static int ToInt32(sbyte value) { return (int)value; }
        public static int ToInt32(byte value) { return (int)value; }
        public static int ToInt32(short value) { return (int)value; }
        public static int ToInt32(ushort value) { return (int)value; }
        public static int ToInt32(int value) { return value; }
        public static int ToInt32(uint value) { if (value > 2147483647) throw Overflow("Int32"); return (int)value; }
        public static int ToInt32(long value) { if (value < int.MinValue || value > 2147483647) throw Overflow("Int32"); return (int)value; }
        public static int ToInt32(ulong value) { if (value > 2147483647) throw Overflow("Int32"); return (int)value; }
        public static int ToInt32(char value) { return (int)value; }
        public static int ToInt32(double value)
        {
            if (!(value > -2147483648.0 - 1.0 && value < 2147483647.0 + 1.0)) throw Overflow("Int32");
            double r = Round(value);
            if (r < -2147483648.0 || r > 2147483647.0) throw Overflow("Int32");
            return (int)r;
        }
        public static int ToInt32(float value) { return ToInt32((double)value); }

        public static uint ToUInt32(object value) { return value == null ? (uint)0 : ToUInt32(value, null); }
        public static uint ToUInt32(object value, IFormatProvider provider)
        {
            if (value == null) return (uint)0;
            string s = value as string;
            if ((object)s != null) return uint.Parse(s, provider);
            if (value is bool) return ToUInt32((bool)value);
            if (value is sbyte) return ToUInt32((sbyte)value);
            if (value is byte) return ToUInt32((byte)value);
            if (value is short) return ToUInt32((short)value);
            if (value is ushort) return ToUInt32((ushort)value);
            if (value is int) return ToUInt32((int)value);
            if (value is uint) return ToUInt32((uint)value);
            if (value is long) return ToUInt32((long)value);
            if (value is ulong) return ToUInt32((ulong)value);
            if (value is char) return ToUInt32((char)value);
            if (value is double) return ToUInt32((double)value);
            if (value is float) return ToUInt32((float)value);
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToUInt32(provider);
            throw new InvalidCastException("Unable to cast object to UInt32.");
        }
        public static uint ToUInt32(string value) { return (object)value == null ? (uint)0 : uint.Parse(value); }
        public static uint ToUInt32(string value, IFormatProvider provider) { return (object)value == null ? (uint)0 : uint.Parse(value, provider); }
        public static uint ToUInt32(string value, int fromBase) { return (uint)ParseBase(value, fromBase, 4, true); }
        public static uint ToUInt32(bool value) { return value ? (uint)1 : (uint)0; }
        public static uint ToUInt32(sbyte value) { if (value < 0) throw Overflow("UInt32"); return (uint)value; }
        public static uint ToUInt32(byte value) { return (uint)value; }
        public static uint ToUInt32(short value) { if (value < 0) throw Overflow("UInt32"); return (uint)value; }
        public static uint ToUInt32(ushort value) { return (uint)value; }
        public static uint ToUInt32(int value) { if (value < 0) throw Overflow("UInt32"); return (uint)value; }
        public static uint ToUInt32(uint value) { return value; }
        public static uint ToUInt32(long value) { if (value < 0 || value > 4294967295) throw Overflow("UInt32"); return (uint)value; }
        public static uint ToUInt32(ulong value) { if (value > 4294967295) throw Overflow("UInt32"); return (uint)value; }
        public static uint ToUInt32(char value) { return (uint)value; }
        public static uint ToUInt32(double value)
        {
            if (!(value > 0.0 - 1.0 && value < 4294967295.0 + 1.0)) throw Overflow("UInt32");
            double r = Round(value);
            if (r < 0.0 || r > 4294967295.0) throw Overflow("UInt32");
            return (uint)r;
        }
        public static uint ToUInt32(float value) { return ToUInt32((double)value); }

        public static long ToInt64(object value) { return value == null ? (long)0 : ToInt64(value, null); }
        public static long ToInt64(object value, IFormatProvider provider)
        {
            if (value == null) return (long)0;
            string s = value as string;
            if ((object)s != null) return long.Parse(s, provider);
            if (value is bool) return ToInt64((bool)value);
            if (value is sbyte) return ToInt64((sbyte)value);
            if (value is byte) return ToInt64((byte)value);
            if (value is short) return ToInt64((short)value);
            if (value is ushort) return ToInt64((ushort)value);
            if (value is int) return ToInt64((int)value);
            if (value is uint) return ToInt64((uint)value);
            if (value is long) return ToInt64((long)value);
            if (value is ulong) return ToInt64((ulong)value);
            if (value is char) return ToInt64((char)value);
            if (value is double) return ToInt64((double)value);
            if (value is float) return ToInt64((float)value);
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToInt64(provider);
            throw new InvalidCastException("Unable to cast object to Int64.");
        }
        public static long ToInt64(string value) { return (object)value == null ? (long)0 : long.Parse(value); }
        public static long ToInt64(string value, IFormatProvider provider) { return (object)value == null ? (long)0 : long.Parse(value, provider); }
        public static long ToInt64(string value, int fromBase) { return (long)ParseBase(value, fromBase, 8, false); }
        public static long ToInt64(bool value) { return value ? (long)1 : (long)0; }
        public static long ToInt64(sbyte value) { return (long)value; }
        public static long ToInt64(byte value) { return (long)value; }
        public static long ToInt64(short value) { return (long)value; }
        public static long ToInt64(ushort value) { return (long)value; }
        public static long ToInt64(int value) { return (long)value; }
        public static long ToInt64(uint value) { return (long)value; }
        public static long ToInt64(long value) { return value; }
        public static long ToInt64(ulong value) { if (value > 9223372036854775807) throw Overflow("Int64"); return (long)value; }
        public static long ToInt64(char value) { return (long)value; }
        public static long ToInt64(double value)
        {
            if (!(value > -9223372036854775808.0 - 1.0 && value < 9223372036854775807.0 + 1.0)) throw Overflow("Int64");
            double r = Round(value);
            if (r < -9223372036854775808.0 || r > 9223372036854775807.0) throw Overflow("Int64");
            return (long)r;
        }
        public static long ToInt64(float value) { return ToInt64((double)value); }

        public static ulong ToUInt64(object value) { return value == null ? (ulong)0 : ToUInt64(value, null); }
        public static ulong ToUInt64(object value, IFormatProvider provider)
        {
            if (value == null) return (ulong)0;
            string s = value as string;
            if ((object)s != null) return ulong.Parse(s, provider);
            if (value is bool) return ToUInt64((bool)value);
            if (value is sbyte) return ToUInt64((sbyte)value);
            if (value is byte) return ToUInt64((byte)value);
            if (value is short) return ToUInt64((short)value);
            if (value is ushort) return ToUInt64((ushort)value);
            if (value is int) return ToUInt64((int)value);
            if (value is uint) return ToUInt64((uint)value);
            if (value is long) return ToUInt64((long)value);
            if (value is ulong) return ToUInt64((ulong)value);
            if (value is char) return ToUInt64((char)value);
            if (value is double) return ToUInt64((double)value);
            if (value is float) return ToUInt64((float)value);
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToUInt64(provider);
            throw new InvalidCastException("Unable to cast object to UInt64.");
        }
        public static ulong ToUInt64(string value) { return (object)value == null ? (ulong)0 : ulong.Parse(value); }
        public static ulong ToUInt64(string value, IFormatProvider provider) { return (object)value == null ? (ulong)0 : ulong.Parse(value, provider); }
        public static ulong ToUInt64(string value, int fromBase) { return (ulong)ParseBase(value, fromBase, 8, true); }
        public static ulong ToUInt64(bool value) { return value ? (ulong)1 : (ulong)0; }
        public static ulong ToUInt64(sbyte value) { if (value < 0) throw Overflow("UInt64"); return (ulong)value; }
        public static ulong ToUInt64(byte value) { return (ulong)value; }
        public static ulong ToUInt64(short value) { if (value < 0) throw Overflow("UInt64"); return (ulong)value; }
        public static ulong ToUInt64(ushort value) { return (ulong)value; }
        public static ulong ToUInt64(int value) { if (value < 0) throw Overflow("UInt64"); return (ulong)value; }
        public static ulong ToUInt64(uint value) { return (ulong)value; }
        public static ulong ToUInt64(long value) { if (value < 0) throw Overflow("UInt64"); return (ulong)value; }
        public static ulong ToUInt64(ulong value) { return value; }
        public static ulong ToUInt64(char value) { return (ulong)value; }
        public static ulong ToUInt64(double value)
        {
            if (!(value > 0.0 - 1.0 && value < 18446744073709551615.0 + 1.0)) throw Overflow("UInt64");
            double r = Round(value);
            if (r < 0.0 || r > 18446744073709551615.0) throw Overflow("UInt64");
            return (ulong)r;
        }
        public static ulong ToUInt64(float value) { return ToUInt64((double)value); }

        public static double ToDouble(object value) { return value == null ? (double)0 : ToDouble(value, null); }
        public static double ToDouble(object value, IFormatProvider provider)
        {
            if (value == null) return (double)0;
            string s = value as string;
            if ((object)s != null) return double.Parse(s, provider);
            if (value is bool) return (bool)value ? (double)1 : (double)0;
            if (value is sbyte) return (double)(sbyte)value;
            if (value is byte) return (double)(byte)value;
            if (value is short) return (double)(short)value;
            if (value is ushort) return (double)(ushort)value;
            if (value is int) return (double)(int)value;
            if (value is uint) return (double)(uint)value;
            if (value is long) return (double)(long)value;
            if (value is ulong) return (double)(ulong)value;
            if (value is char) return (double)(char)value;
            if (value is double) return (double)(double)value;
            if (value is float) return (double)(float)value;
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToDouble(provider);
            throw new InvalidCastException("Unable to cast object to Double.");
        }
        public static double ToDouble(string value) { return (object)value == null ? (double)0 : double.Parse(value); }
        public static double ToDouble(string value, IFormatProvider provider) { return (object)value == null ? (double)0 : double.Parse(value, provider); }
        public static double ToDouble(bool value) { return value ? (double)1 : (double)0; }
        public static double ToDouble(sbyte value) { return (double)value; }
        public static double ToDouble(byte value) { return (double)value; }
        public static double ToDouble(short value) { return (double)value; }
        public static double ToDouble(ushort value) { return (double)value; }
        public static double ToDouble(int value) { return (double)value; }
        public static double ToDouble(uint value) { return (double)value; }
        public static double ToDouble(long value) { return (double)value; }
        public static double ToDouble(ulong value) { return (double)value; }
        public static double ToDouble(double value) { return (double)value; }
        public static double ToDouble(float value) { return (double)value; }

        public static float ToSingle(object value) { return value == null ? (float)0 : ToSingle(value, null); }
        public static float ToSingle(object value, IFormatProvider provider)
        {
            if (value == null) return (float)0;
            string s = value as string;
            if ((object)s != null) return float.Parse(s, provider);
            if (value is bool) return (bool)value ? (float)1 : (float)0;
            if (value is sbyte) return (float)(sbyte)value;
            if (value is byte) return (float)(byte)value;
            if (value is short) return (float)(short)value;
            if (value is ushort) return (float)(ushort)value;
            if (value is int) return (float)(int)value;
            if (value is uint) return (float)(uint)value;
            if (value is long) return (float)(long)value;
            if (value is ulong) return (float)(ulong)value;
            if (value is char) return (float)(char)value;
            if (value is double) return (float)(double)value;
            if (value is float) return (float)(float)value;
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToSingle(provider);
            throw new InvalidCastException("Unable to cast object to Single.");
        }
        public static float ToSingle(string value) { return (object)value == null ? (float)0 : float.Parse(value); }
        public static float ToSingle(string value, IFormatProvider provider) { return (object)value == null ? (float)0 : float.Parse(value, provider); }
        public static float ToSingle(bool value) { return value ? (float)1 : (float)0; }
        public static float ToSingle(sbyte value) { return (float)value; }
        public static float ToSingle(byte value) { return (float)value; }
        public static float ToSingle(short value) { return (float)value; }
        public static float ToSingle(ushort value) { return (float)value; }
        public static float ToSingle(int value) { return (float)value; }
        public static float ToSingle(uint value) { return (float)value; }
        public static float ToSingle(long value) { return (float)value; }
        public static float ToSingle(ulong value) { return (float)value; }
        public static float ToSingle(double value) { return (float)value; }
        public static float ToSingle(float value) { return (float)value; }

        public static bool ToBoolean(object value) { return value != null && ToBoolean(value, null); }
        public static bool ToBoolean(object value, IFormatProvider provider)
        {
            if (value == null) return false;
            string s = value as string;
            if ((object)s != null) return bool.Parse(s);
            if (value is bool) return (bool)value;
            if (value is sbyte) return (sbyte)value != 0;
            if (value is byte) return (byte)value != 0;
            if (value is short) return (short)value != 0;
            if (value is ushort) return (ushort)value != 0;
            if (value is int) return (int)value != 0;
            if (value is uint) return (uint)value != 0;
            if (value is long) return (long)value != 0;
            if (value is ulong) return (ulong)value != 0;
            if (value is double) return (double)value != 0;
            if (value is float) return (float)value != 0;
            IConvertible c = value as IConvertible;
            if (c != null) return c.ToBoolean(provider);
            throw new InvalidCastException("Unable to cast object to Boolean.");
        }
        public static bool ToBoolean(string value) { return (object)value != null && bool.Parse(value); }
        public static bool ToBoolean(string value, IFormatProvider provider) { return ToBoolean(value); }
        public static bool ToBoolean(bool value) { return value; }
        public static bool ToBoolean(sbyte value) { return value != 0; }
        public static bool ToBoolean(byte value) { return value != 0; }
        public static bool ToBoolean(short value) { return value != 0; }
        public static bool ToBoolean(ushort value) { return value != 0; }
        public static bool ToBoolean(int value) { return value != 0; }
        public static bool ToBoolean(uint value) { return value != 0; }
        public static bool ToBoolean(long value) { return value != 0; }
        public static bool ToBoolean(ulong value) { return value != 0; }
        public static bool ToBoolean(double value) { return value != 0; }
        public static bool ToBoolean(float value) { return value != 0; }

        public static char ToChar(object value) { return value == null ? '\0' : ToChar(value, null); }
        public static char ToChar(object value, IFormatProvider provider)
        {
            if (value == null) return '\0';
            string s = value as string;
            if ((object)s != null) return ToChar(s);
            if (value is char) return (char)value;
            if (value is sbyte) return ToChar((sbyte)value);
            if (value is byte) return ToChar((byte)value);
            if (value is short) return ToChar((short)value);
            if (value is ushort) return ToChar((ushort)value);
            if (value is int) return ToChar((int)value);
            if (value is uint) return ToChar((uint)value);
            if (value is long) return ToChar((long)value);
            if (value is ulong) return ToChar((ulong)value);
            throw new InvalidCastException("Unable to cast object to Char.");
        }
        public static char ToChar(string value)
        {
            if ((object)value == null) throw new ArgumentNullException("value");
            if (value.Length != 1) throw new FormatException("String must be exactly one character long.");
            return value[0];
        }
        public static char ToChar(string value, IFormatProvider provider) { return ToChar(value); }
        public static char ToChar(char value) { return value; }
        public static char ToChar(sbyte value) { if (value < 0) throw Overflow("Char"); return (char)value; }
        public static char ToChar(byte value) { return (char)value; }
        public static char ToChar(short value) { if (value < 0) throw Overflow("Char"); return (char)value; }
        public static char ToChar(ushort value) { return (char)value; }
        public static char ToChar(int value) { if (value < 0 || value > 65535) throw Overflow("Char"); return (char)value; }
        public static char ToChar(uint value) { if (value > 65535) throw Overflow("Char"); return (char)value; }
        public static char ToChar(long value) { if (value < 0 || value > 65535) throw Overflow("Char"); return (char)value; }
        public static char ToChar(ulong value) { if (value > 65535) throw Overflow("Char"); return (char)value; }

        public static string ToString(object value) { return ToString(value, null); }
        public static string ToString(object value, IFormatProvider provider)
        {
            if (value == null) return string.Empty;
            IFormattable f = value as IFormattable;
            if (f != null && !(value is string)) return f.ToString(null, provider);
            string s = value.ToString();
            return (object)s == null ? string.Empty : s;
        }
        public static string ToString(string value) { return value; }
        public static string ToString(bool value) { return value.ToString(); }
        public static string ToString(char value) { return value.ToString(); }
        public static string ToString(sbyte value) { return value.ToString(); }
        public static string ToString(sbyte value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(byte value) { return value.ToString(); }
        public static string ToString(byte value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(short value) { return value.ToString(); }
        public static string ToString(short value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(ushort value) { return value.ToString(); }
        public static string ToString(ushort value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(int value) { return value.ToString(); }
        public static string ToString(int value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(uint value) { return value.ToString(); }
        public static string ToString(uint value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(long value) { return value.ToString(); }
        public static string ToString(long value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(ulong value) { return value.ToString(); }
        public static string ToString(ulong value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(double value) { return value.ToString(); }
        public static string ToString(double value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(float value) { return value.ToString(); }
        public static string ToString(float value, IFormatProvider provider) { return value.ToString(provider); }
        public static string ToString(byte value, int toBase) { return ToBase((ulong)value, toBase, 1, value < 0); }
        public static string ToString(short value, int toBase) { return ToBase((ulong)value, toBase, 2, value < 0); }
        public static string ToString(int value, int toBase) { return ToBase((ulong)value, toBase, 4, value < 0); }
        public static string ToString(long value, int toBase) { return ToBase((ulong)value, toBase, 8, value < 0); }
    }
}
