// BCL Winlux : System.Text.Encoding, UTF8Encoding, UnicodeEncoding,
// UTF32Encoding, ASCIIEncoding, Latin-1 et Windows-1252 (#1963). Licence
// GPL 3. Écrit d'après la documentation publique de l'API (« Encoding
// Class », « Encoding.GetEncoding »), les RFC 3629 (UTF-8) et 2781 (UTF-16)
// et la table publique de la page de code 1252 publiée par l'Unicode
// Consortium ; aucun code de Microsoft, de Mono ou de Wine.
//
// Une séquence invalide est remplacée par U+FFFD (ou « ? » pour les
// encodages à un octet), comme le font par défaut les encodeurs .NET.

namespace System.Text
{
    public abstract class Encoding
    {
        private static Encoding _utf8;
        private static Encoding _utf8NoBom;
        private static Encoding _unicode;
        private static Encoding _bigEndian;
        private static Encoding _utf32;
        private static Encoding _ascii;
        private static Encoding _latin1;
        private static Encoding _ansi;

        private readonly int _codePage;

        protected Encoding() : this(0) { }

        protected Encoding(int codePage)
        {
            _codePage = codePage;
        }

        public virtual int CodePage { get { return _codePage; } }
        public abstract string WebName { get; }
        public virtual string EncodingName { get { return WebName; } }
        public virtual string BodyName { get { return WebName; } }
        public virtual string HeaderName { get { return WebName; } }
        public virtual bool IsSingleByte { get { return false; } }

        // UTF-8 avec marque d'ordre, comme Encoding.UTF8.
        public static Encoding UTF8 { get { if (_utf8 == null) _utf8 = new UTF8Encoding(true); return _utf8; } }
        public static Encoding Unicode { get { if (_unicode == null) _unicode = new UnicodeEncoding(false, true); return _unicode; } }
        public static Encoding BigEndianUnicode { get { if (_bigEndian == null) _bigEndian = new UnicodeEncoding(true, true); return _bigEndian; } }
        public static Encoding UTF32 { get { if (_utf32 == null) _utf32 = new UTF32Encoding(false, true); return _utf32; } }
        public static Encoding ASCII { get { if (_ascii == null) _ascii = new ASCIIEncoding(); return _ascii; } }

        // Page de code ANSI d'un Windows occidental : 1252.
        public static Encoding Default { get { if (_ansi == null) _ansi = new SingleByteEncoding(1252); return _ansi; } }

        public static Encoding GetEncoding(int codepage)
        {
            switch (codepage)
            {
                case 0:
                case 1252:
                    return Default;
                case 65001:
                    return UTF8;
                case 1200:
                    return Unicode;
                case 1201:
                    return BigEndianUnicode;
                case 12000:
                    return UTF32;
                case 20127:
                    return ASCII;
                case 28591:
                    if (_latin1 == null) _latin1 = new SingleByteEncoding(28591);
                    return _latin1;
            }
            throw new NotSupportedException(string.Concat("No data is available for encoding ", codepage.ToString(), "."));
        }

        public static Encoding GetEncoding(string name)
        {
            if ((object)name == null)
                throw new ArgumentNullException("name");
            string n = name.ToLowerInvariant();
            if (n == "utf-8" || n == "utf8") return UTF8;
            if (n == "utf-16" || n == "unicode" || n == "utf-16le") return Unicode;
            if (n == "utf-16be" || n == "unicodefffe") return BigEndianUnicode;
            if (n == "utf-32" || n == "utf-32le") return UTF32;
            if (n == "us-ascii" || n == "ascii") return ASCII;
            if (n == "iso-8859-1" || n == "latin1") return GetEncoding(28591);
            if (n == "windows-1252") return GetEncoding(1252);
            throw new ArgumentException(string.Concat("'", name, "' is not a supported encoding name."));
        }

        internal static Encoding Utf8NoBom
        {
            get { if (_utf8NoBom == null) _utf8NoBom = new UTF8Encoding(false); return _utf8NoBom; }
        }

        public virtual byte[] GetPreamble() { return new byte[0]; }

        public abstract int GetByteCount(char[] chars, int index, int count);
        public abstract int GetBytes(char[] chars, int charIndex, int charCount, byte[] bytes, int byteIndex);
        public abstract int GetCharCount(byte[] bytes, int index, int count);
        public abstract int GetChars(byte[] bytes, int byteIndex, int byteCount, char[] chars, int charIndex);
        public abstract int GetMaxByteCount(int charCount);
        public abstract int GetMaxCharCount(int byteCount);

        private static void Check(object array, int index, int count, int length, string name)
        {
            if (array == null)
                throw new ArgumentNullException(name);
            if (index < 0 || count < 0 || index > length - count)
                throw new ArgumentOutOfRangeException("index");
        }

        public virtual int GetByteCount(char[] chars)
        {
            if (chars == null)
                throw new ArgumentNullException("chars");
            return GetByteCount(chars, 0, chars.Length);
        }

        public virtual int GetByteCount(string s)
        {
            if ((object)s == null)
                throw new ArgumentNullException("s");
            return GetByteCount(s.ToCharArray());
        }

        public virtual byte[] GetBytes(char[] chars)
        {
            if (chars == null)
                throw new ArgumentNullException("chars");
            return GetBytes(chars, 0, chars.Length);
        }

        public virtual byte[] GetBytes(char[] chars, int index, int count)
        {
            Check(chars, index, count, chars == null ? 0 : chars.Length, "chars");
            byte[] bytes = new byte[GetByteCount(chars, index, count)];
            GetBytes(chars, index, count, bytes, 0);
            return bytes;
        }

        public virtual byte[] GetBytes(string s)
        {
            if ((object)s == null)
                throw new ArgumentNullException("s");
            return GetBytes(s.ToCharArray());
        }

        public virtual int GetBytes(string s, int charIndex, int charCount, byte[] bytes, int byteIndex)
        {
            if ((object)s == null)
                throw new ArgumentNullException("s");
            return GetBytes(s.ToCharArray(), charIndex, charCount, bytes, byteIndex);
        }

        public virtual int GetCharCount(byte[] bytes)
        {
            if (bytes == null)
                throw new ArgumentNullException("bytes");
            return GetCharCount(bytes, 0, bytes.Length);
        }

        public virtual char[] GetChars(byte[] bytes)
        {
            if (bytes == null)
                throw new ArgumentNullException("bytes");
            return GetChars(bytes, 0, bytes.Length);
        }

        public virtual char[] GetChars(byte[] bytes, int index, int count)
        {
            Check(bytes, index, count, bytes == null ? 0 : bytes.Length, "bytes");
            char[] chars = new char[GetCharCount(bytes, index, count)];
            GetChars(bytes, index, count, chars, 0);
            return chars;
        }

        public virtual string GetString(byte[] bytes)
        {
            if (bytes == null)
                throw new ArgumentNullException("bytes");
            return GetString(bytes, 0, bytes.Length);
        }

        public virtual string GetString(byte[] bytes, int index, int count)
        {
            return new string(GetChars(bytes, index, count));
        }

        public override bool Equals(object value)
        {
            Encoding e = value as Encoding;
            return e != null && e.CodePage == CodePage && e.GetType() == GetType();
        }

        public override int GetHashCode() { return CodePage; }
    }

    public class UTF8Encoding : Encoding
    {
        private readonly bool _bom;
        private readonly bool _throw;

        public UTF8Encoding() : this(false, false) { }
        public UTF8Encoding(bool encoderShouldEmitUTF8Identifier) : this(encoderShouldEmitUTF8Identifier, false) { }

        public UTF8Encoding(bool encoderShouldEmitUTF8Identifier, bool throwOnInvalidBytes) : base(65001)
        {
            _bom = encoderShouldEmitUTF8Identifier;
            _throw = throwOnInvalidBytes;
        }

        public override string WebName { get { return "utf-8"; } }
        public override string EncodingName { get { return "Unicode (UTF-8)"; } }
        public override byte[] GetPreamble() { return _bom ? new byte[] { 0xEF, 0xBB, 0xBF } : new byte[0]; }
        public override int GetMaxByteCount(int charCount) { return (charCount + 1) * 3; }
        public override int GetMaxCharCount(int byteCount) { return byteCount + 1; }

        // Point de code au rang i ; une demi-paire isolée devient U+FFFD.
        private static int CodePoint(char[] chars, ref int i, int end)
        {
            int c = chars[i];
            if (c >= 0xD800 && c <= 0xDBFF && i + 1 < end && chars[i + 1] >= 0xDC00 && chars[i + 1] <= 0xDFFF)
            {
                i++;
                return 0x10000 + ((c - 0xD800) << 10) + (chars[i] - 0xDC00);
            }
            if (c >= 0xD800 && c <= 0xDFFF)
                return 0xFFFD;
            return c;
        }

        public override int GetByteCount(char[] chars, int index, int count)
        {
            if (chars == null)
                throw new ArgumentNullException("chars");
            int n = 0;
            int end = index + count;
            for (int i = index; i < end; i++)
            {
                int cp = CodePoint(chars, ref i, end);
                n += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
            }
            return n;
        }

        public override int GetBytes(char[] chars, int charIndex, int charCount, byte[] bytes, int byteIndex)
        {
            if (chars == null)
                throw new ArgumentNullException("chars");
            if (bytes == null)
                throw new ArgumentNullException("bytes");
            int o = byteIndex;
            int end = charIndex + charCount;
            for (int i = charIndex; i < end; i++)
            {
                int cp = CodePoint(chars, ref i, end);
                if (cp < 0x80)
                {
                    bytes[o++] = (byte)cp;
                }
                else if (cp < 0x800)
                {
                    bytes[o++] = (byte)(0xC0 | (cp >> 6));
                    bytes[o++] = (byte)(0x80 | (cp & 0x3F));
                }
                else if (cp < 0x10000)
                {
                    bytes[o++] = (byte)(0xE0 | (cp >> 12));
                    bytes[o++] = (byte)(0x80 | ((cp >> 6) & 0x3F));
                    bytes[o++] = (byte)(0x80 | (cp & 0x3F));
                }
                else
                {
                    bytes[o++] = (byte)(0xF0 | (cp >> 18));
                    bytes[o++] = (byte)(0x80 | ((cp >> 12) & 0x3F));
                    bytes[o++] = (byte)(0x80 | ((cp >> 6) & 0x3F));
                    bytes[o++] = (byte)(0x80 | (cp & 0x3F));
                }
            }
            return o - byteIndex;
        }

        // Décode un point de code à partir de `i` (avancé) ; -1 : invalide,
        // un seul octet est alors consommé (RFC 3629, §3 et §10).
        private static int Decode(byte[] bytes, ref int i, int end)
        {
            int b = bytes[i++];
            if (b < 0x80)
                return b;
            int need;
            int cp;
            int min;
            if (b >= 0xC2 && b <= 0xDF) { need = 1; cp = b & 0x1F; min = 0x80; }
            else if (b >= 0xE0 && b <= 0xEF) { need = 2; cp = b & 0x0F; min = 0x800; }
            else if (b >= 0xF0 && b <= 0xF4) { need = 3; cp = b & 0x07; min = 0x10000; }
            else return -1;
            int start = i;
            for (int k = 0; k < need; k++)
            {
                if (i >= end || (bytes[i] & 0xC0) != 0x80)
                {
                    i = start;
                    return -1;
                }
                cp = (cp << 6) | (bytes[i++] & 0x3F);
            }
            if (cp < min || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF))
            {
                i = start;
                return -1;
            }
            return cp;
        }

        private int Invalid()
        {
            if (_throw)
                throw new ArgumentException("Invalid byte sequence in UTF-8 input.");
            return 0xFFFD;
        }

        public override int GetCharCount(byte[] bytes, int index, int count)
        {
            if (bytes == null)
                throw new ArgumentNullException("bytes");
            int n = 0;
            int end = index + count;
            int i = index;
            while (i < end)
            {
                int cp = Decode(bytes, ref i, end);
                if (cp < 0)
                    cp = Invalid();
                n += cp >= 0x10000 ? 2 : 1;
            }
            return n;
        }

        public override int GetChars(byte[] bytes, int byteIndex, int byteCount, char[] chars, int charIndex)
        {
            if (bytes == null)
                throw new ArgumentNullException("bytes");
            if (chars == null)
                throw new ArgumentNullException("chars");
            int o = charIndex;
            int end = byteIndex + byteCount;
            int i = byteIndex;
            while (i < end)
            {
                int cp = Decode(bytes, ref i, end);
                if (cp < 0)
                    cp = Invalid();
                if (cp >= 0x10000)
                {
                    cp -= 0x10000;
                    chars[o++] = (char)(0xD800 + (cp >> 10));
                    chars[o++] = (char)(0xDC00 + (cp & 0x3FF));
                }
                else
                {
                    chars[o++] = (char)cp;
                }
            }
            return o - charIndex;
        }
    }

    public class UnicodeEncoding : Encoding
    {
        private readonly bool _bigEndian;
        private readonly bool _bom;

        public UnicodeEncoding() : this(false, true) { }
        public UnicodeEncoding(bool bigEndian, bool byteOrderMark) : this(bigEndian, byteOrderMark, false) { }

        public UnicodeEncoding(bool bigEndian, bool byteOrderMark, bool throwOnInvalidBytes) : base(bigEndian ? 1201 : 1200)
        {
            _bigEndian = bigEndian;
            _bom = byteOrderMark;
        }

        public override string WebName { get { return _bigEndian ? "utf-16BE" : "utf-16"; } }
        public override string EncodingName { get { return _bigEndian ? "Unicode (Big-Endian)" : "Unicode"; } }

        public override byte[] GetPreamble()
        {
            if (!_bom)
                return new byte[0];
            return _bigEndian ? new byte[] { 0xFE, 0xFF } : new byte[] { 0xFF, 0xFE };
        }

        public override int GetMaxByteCount(int charCount) { return (charCount + 1) * 2; }
        public override int GetMaxCharCount(int byteCount) { return byteCount / 2 + 2; }
        public override int GetByteCount(char[] chars, int index, int count) { return count * 2; }

        public override int GetBytes(char[] chars, int charIndex, int charCount, byte[] bytes, int byteIndex)
        {
            for (int i = 0; i < charCount; i++)
            {
                int c = chars[charIndex + i];
                bytes[byteIndex + 2 * i] = (byte)(_bigEndian ? c >> 8 : c);
                bytes[byteIndex + 2 * i + 1] = (byte)(_bigEndian ? c : c >> 8);
            }
            return charCount * 2;
        }

        // Un octet final isolé devient U+FFFD.
        public override int GetCharCount(byte[] bytes, int index, int count) { return count / 2 + count % 2; }

        public override int GetChars(byte[] bytes, int byteIndex, int byteCount, char[] chars, int charIndex)
        {
            int n = byteCount / 2;
            for (int i = 0; i < n; i++)
            {
                int a = bytes[byteIndex + 2 * i];
                int b = bytes[byteIndex + 2 * i + 1];
                chars[charIndex + i] = (char)(_bigEndian ? (a << 8) | b : (b << 8) | a);
            }
            if (byteCount % 2 != 0)
                chars[charIndex + n++] = '�';
            return n;
        }
    }

    public sealed class UTF32Encoding : Encoding
    {
        private readonly bool _bigEndian;
        private readonly bool _bom;

        public UTF32Encoding() : this(false, true) { }

        public UTF32Encoding(bool bigEndian, bool byteOrderMark) : base(bigEndian ? 12001 : 12000)
        {
            _bigEndian = bigEndian;
            _bom = byteOrderMark;
        }

        public override string WebName { get { return _bigEndian ? "utf-32BE" : "utf-32"; } }

        public override byte[] GetPreamble()
        {
            if (!_bom)
                return new byte[0];
            return _bigEndian ? new byte[] { 0, 0, 0xFE, 0xFF } : new byte[] { 0xFF, 0xFE, 0, 0 };
        }

        public override int GetMaxByteCount(int charCount) { return (charCount + 1) * 4; }
        public override int GetMaxCharCount(int byteCount) { return byteCount / 2 + 2; }

        public override int GetByteCount(char[] chars, int index, int count)
        {
            int n = 0;
            for (int i = index; i < index + count; i++)
            {
                if (chars[i] >= 0xD800 && chars[i] <= 0xDBFF && i + 1 < index + count && chars[i + 1] >= 0xDC00 && chars[i + 1] <= 0xDFFF)
                    i++;
                n += 4;
            }
            return n;
        }

        public override int GetBytes(char[] chars, int charIndex, int charCount, byte[] bytes, int byteIndex)
        {
            int o = byteIndex;
            int end = charIndex + charCount;
            for (int i = charIndex; i < end; i++)
            {
                int cp = chars[i];
                if (cp >= 0xD800 && cp <= 0xDBFF && i + 1 < end && chars[i + 1] >= 0xDC00 && chars[i + 1] <= 0xDFFF)
                    cp = 0x10000 + ((cp - 0xD800) << 10) + (chars[++i] - 0xDC00);
                else if (cp >= 0xD800 && cp <= 0xDFFF)
                    cp = 0xFFFD;
                for (int k = 0; k < 4; k++)
                    bytes[o + (_bigEndian ? 3 - k : k)] = (byte)(cp >> (8 * k));
                o += 4;
            }
            return o - byteIndex;
        }

        private int At(byte[] bytes, int i)
        {
            int cp = 0;
            for (int k = 0; k < 4; k++)
                cp |= bytes[i + (_bigEndian ? 3 - k : k)] << (8 * k);
            return cp < 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF) ? 0xFFFD : cp;
        }

        public override int GetCharCount(byte[] bytes, int index, int count)
        {
            int n = 0;
            for (int i = index; i + 3 < index + count; i += 4)
                n += At(bytes, i) >= 0x10000 ? 2 : 1;
            return n + (count % 4 != 0 ? 1 : 0);
        }

        public override int GetChars(byte[] bytes, int byteIndex, int byteCount, char[] chars, int charIndex)
        {
            int o = charIndex;
            int i = byteIndex;
            for (; i + 3 < byteIndex + byteCount; i += 4)
            {
                int cp = At(bytes, i);
                if (cp >= 0x10000)
                {
                    cp -= 0x10000;
                    chars[o++] = (char)(0xD800 + (cp >> 10));
                    chars[o++] = (char)(0xDC00 + (cp & 0x3FF));
                }
                else
                {
                    chars[o++] = (char)cp;
                }
            }
            if (byteCount % 4 != 0)
                chars[o++] = '�';
            return o - charIndex;
        }
    }

    // Encodage à un octet par caractère : ASCII (20127), Latin-1 (28591) et
    // Windows-1252 (1252). Un caractère sans octet devient « ? ».
    internal class SingleByteEncoding : Encoding
    {
        // Caractères de 0x80 à 0x9F en 1252 ; 0 : octet non attribué.
        private static readonly char[] Cp1252High =
        {
            '€', '\0', '‚', 'ƒ', '„', '…', '†', '‡',
            'ˆ', '‰', 'Š', '‹', 'Œ', '\0', 'Ž', '\0',
            '\0', '‘', '’', '“', '”', '•', '–', '—',
            '˜', '™', 'š', '›', 'œ', '\0', 'ž', 'Ÿ',
        };

        internal SingleByteEncoding(int codePage) : base(codePage) { }

        public override string WebName
        {
            get { return CodePage == 20127 ? "us-ascii" : CodePage == 28591 ? "iso-8859-1" : "Windows-1252"; }
        }

        public override bool IsSingleByte { get { return true; } }
        public override int GetMaxByteCount(int charCount) { return charCount + 1; }
        public override int GetMaxCharCount(int byteCount) { return byteCount; }
        public override int GetByteCount(char[] chars, int index, int count) { return count; }
        public override int GetCharCount(byte[] bytes, int index, int count) { return count; }

        private byte ToByte(char c)
        {
            if (c < 0x80)
                return (byte)c;
            if (CodePage == 20127)
                return (byte)'?';
            if (CodePage == 28591)
                return c <= 0xFF ? (byte)c : (byte)'?';
            if (c >= 0xA0 && c <= 0xFF)
                return (byte)c;
            for (int i = 0; i < Cp1252High.Length; i++)
            {
                if (Cp1252High[i] == c && c != '\0')
                    return (byte)(0x80 + i);
            }
            return (byte)'?';
        }

        private char ToChar(byte b)
        {
            if (b < 0x80)
                return (char)b;
            if (CodePage == 20127)
                return '?';
            if (CodePage == 1252 && b < 0xA0)
            {
                char c = Cp1252High[b - 0x80];
                return c == '\0' ? (char)b : c;
            }
            return (char)b;
        }

        public override int GetBytes(char[] chars, int charIndex, int charCount, byte[] bytes, int byteIndex)
        {
            for (int i = 0; i < charCount; i++)
                bytes[byteIndex + i] = ToByte(chars[charIndex + i]);
            return charCount;
        }

        public override int GetChars(byte[] bytes, int byteIndex, int byteCount, char[] chars, int charIndex)
        {
            for (int i = 0; i < byteCount; i++)
                chars[charIndex + i] = ToChar(bytes[byteIndex + i]);
            return byteCount;
        }
    }

    public class ASCIIEncoding : Encoding
    {
        public ASCIIEncoding() : base(20127) { }

        public override string WebName { get { return "us-ascii"; } }
        public override bool IsSingleByte { get { return true; } }
        public override int GetMaxByteCount(int charCount) { return charCount + 1; }
        public override int GetMaxCharCount(int byteCount) { return byteCount; }
        public override int GetByteCount(char[] chars, int index, int count) { return count; }
        public override int GetCharCount(byte[] bytes, int index, int count) { return count; }

        public override int GetBytes(char[] chars, int charIndex, int charCount, byte[] bytes, int byteIndex)
        {
            for (int i = 0; i < charCount; i++)
            {
                char c = chars[charIndex + i];
                bytes[byteIndex + i] = c < 0x80 ? (byte)c : (byte)'?';
            }
            return charCount;
        }

        public override int GetChars(byte[] bytes, int byteIndex, int byteCount, char[] chars, int charIndex)
        {
            for (int i = 0; i < byteCount; i++)
            {
                byte b = bytes[byteIndex + i];
                chars[charIndex + i] = b < 0x80 ? (char)b : '?';
            }
            return byteCount;
        }
    }
}
