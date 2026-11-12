// Lecteurs et écrivains de texte de la mscorlib Winlux (#2012) :
// TextWriter, TextReader, StringWriter, StringReader, StreamWriter et
// StreamReader. Licence GPL 3. Écrit d'après la documentation publique de
// .NET Framework 4.x (MS Learn : « TextWriter Class », « StreamReader
// Class », « StreamReader(Stream, Encoding, Boolean) ») et les marques
// d'ordre des octets d'Unicode (FAQ « UTF-8, UTF-16, UTF-32 & BOM ») ;
// aucun code de Microsoft, de Mono ou de Wine.
using System.Text;
using System.Threading.Tasks;

namespace System.IO
{
    public abstract class TextWriter : IDisposable
    {
        public static readonly TextWriter Null = new EcrivainNul();

        protected char[] CoreNewLine = new char[] { '\r', '\n' };
        private readonly IFormatProvider _format;

        protected TextWriter() { }
        protected TextWriter(IFormatProvider formatProvider) { _format = formatProvider; }

        public abstract Encoding Encoding { get; }
        public virtual IFormatProvider FormatProvider { get { return _format; } }

        public virtual string NewLine
        {
            get { return new string(CoreNewLine); }
            set { CoreNewLine = (value ?? "\r\n").ToCharArray(); }
        }

        public virtual void Close() { Dispose(true); }
        public void Dispose() { Dispose(true); }
        protected virtual void Dispose(bool disposing) { }
        public virtual void Flush() { }

        public static TextWriter Synchronized(TextWriter writer)
        {
            if (writer == null) throw new ArgumentNullException("writer");
            return writer;
        }

        public virtual void Write(char value) { }

        public virtual void Write(char[] buffer)
        {
            if (buffer != null) Write(buffer, 0, buffer.Length);
        }

        public virtual void Write(char[] buffer, int index, int count)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            if (index < 0) throw new ArgumentOutOfRangeException("index", "Non-negative number required.");
            if (count < 0) throw new ArgumentOutOfRangeException("count", "Non-negative number required.");
            if (buffer.Length - index < count) throw new ArgumentException("Offset and length were out of bounds for the array or count is greater than the number of elements from index to the end of the source collection.");
            for (int i = 0; i < count; i++) Write(buffer[index + i]);
        }

        public virtual void Write(string value)
        {
            if (value != null) Write(value.ToCharArray());
        }

        public virtual void Write(bool value) { Write(value ? "True" : "False"); }
        public virtual void Write(int value) { Write(value.ToString()); }
        public virtual void Write(uint value) { Write(value.ToString()); }
        public virtual void Write(long value) { Write(value.ToString()); }
        public virtual void Write(ulong value) { Write(value.ToString()); }
        public virtual void Write(float value) { Write(value.ToString()); }
        public virtual void Write(double value) { Write(value.ToString()); }
        public virtual void Write(object value) { if (value != null) Write(value.ToString()); }
        public virtual void Write(string format, object arg0) { Write(string.Format(format, arg0)); }
        public virtual void Write(string format, object arg0, object arg1) { Write(string.Format(format, arg0, arg1)); }
        public virtual void Write(string format, object arg0, object arg1, object arg2) { Write(string.Format(format, arg0, arg1, arg2)); }
        public virtual void Write(string format, params object[] arg) { Write(string.Format(format, arg)); }

        public virtual void WriteLine() { Write(CoreNewLine); }
        public virtual void WriteLine(char value) { Write(value); WriteLine(); }
        public virtual void WriteLine(char[] buffer) { Write(buffer); WriteLine(); }
        public virtual void WriteLine(char[] buffer, int index, int count) { Write(buffer, index, count); WriteLine(); }
        public virtual void WriteLine(string value) { Write(value); WriteLine(); }
        public virtual void WriteLine(bool value) { Write(value); WriteLine(); }
        public virtual void WriteLine(int value) { Write(value); WriteLine(); }
        public virtual void WriteLine(uint value) { Write(value); WriteLine(); }
        public virtual void WriteLine(long value) { Write(value); WriteLine(); }
        public virtual void WriteLine(ulong value) { Write(value); WriteLine(); }
        public virtual void WriteLine(float value) { Write(value); WriteLine(); }
        public virtual void WriteLine(double value) { Write(value); WriteLine(); }
        public virtual void WriteLine(object value) { Write(value); WriteLine(); }
        public virtual void WriteLine(string format, object arg0) { WriteLine(string.Format(format, arg0)); }
        public virtual void WriteLine(string format, object arg0, object arg1) { WriteLine(string.Format(format, arg0, arg1)); }
        public virtual void WriteLine(string format, object arg0, object arg1, object arg2) { WriteLine(string.Format(format, arg0, arg1, arg2)); }
        public virtual void WriteLine(string format, params object[] arg) { WriteLine(string.Format(format, arg)); }

        public virtual Task WriteAsync(char value) { return Faire(delegate { Write(value); }); }
        public virtual Task WriteAsync(string value) { return Faire(delegate { Write(value); }); }
        public Task WriteAsync(char[] buffer) { return Faire(delegate { Write(buffer); }); }
        public virtual Task WriteAsync(char[] buffer, int index, int count) { return Faire(delegate { Write(buffer, index, count); }); }
        public virtual Task WriteLineAsync() { return Faire(delegate { WriteLine(); }); }
        public virtual Task WriteLineAsync(char value) { return Faire(delegate { WriteLine(value); }); }
        public virtual Task WriteLineAsync(string value) { return Faire(delegate { WriteLine(value); }); }
        public Task WriteLineAsync(char[] buffer) { return Faire(delegate { WriteLine(buffer); }); }
        public virtual Task WriteLineAsync(char[] buffer, int index, int count) { return Faire(delegate { WriteLine(buffer, index, count); }); }
        public virtual Task FlushAsync() { return Faire(delegate { Flush(); }); }

        internal static Task Faire(Action action)
        {
            try { action(); return Task.CompletedTask; }
            catch (Exception e) { return Task.FromException(e); }
        }

        private sealed class EcrivainNul : TextWriter
        {
            public override Encoding Encoding { get { return Encoding.Default; } }
            public override void Write(char value) { }
            public override void Write(string value) { }
            public override void Write(char[] buffer, int index, int count) { }
        }
    }

    public abstract class TextReader : IDisposable
    {
        public static readonly TextReader Null = new LecteurNul();

        protected TextReader() { }

        public virtual void Close() { Dispose(true); }
        public void Dispose() { Dispose(true); }
        protected virtual void Dispose(bool disposing) { }

        public virtual int Peek() { return -1; }
        public virtual int Read() { return -1; }

        public virtual int Read(char[] buffer, int index, int count)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            if (index < 0 || count < 0) throw new ArgumentOutOfRangeException(index < 0 ? "index" : "count", "Non-negative number required.");
            if (buffer.Length - index < count) throw new ArgumentException("Offset and length were out of bounds for the array or count is greater than the number of elements from index to the end of the source collection.");
            int n = 0;
            while (n < count)
            {
                int c = Read();
                if (c < 0) break;
                buffer[index + n++] = (char)c;
            }
            return n;
        }

        public virtual int ReadBlock(char[] buffer, int index, int count)
        {
            int total = 0, n;
            while (total < count && (n = Read(buffer, index + total, count - total)) > 0) total += n;
            return total;
        }

        public virtual string ReadToEnd()
        {
            StringBuilder sb = new StringBuilder();
            char[] tampon = new char[4096];
            int n;
            while ((n = Read(tampon, 0, tampon.Length)) > 0) sb.Append(tampon, 0, n);
            return sb.ToString();
        }

        // Une ligne se termine par « \n », « \r » ou « \r\n ».
        public virtual string ReadLine()
        {
            StringBuilder sb = new StringBuilder();
            while (true)
            {
                int c = Read();
                if (c < 0) return sb.Length > 0 ? sb.ToString() : null;
                if (c == '\r')
                {
                    if (Peek() == '\n') Read();
                    return sb.ToString();
                }
                if (c == '\n') return sb.ToString();
                sb.Append((char)c);
            }
        }

        public virtual Task<string> ReadToEndAsync() { return Faire(ReadToEnd); }
        public virtual Task<string> ReadLineAsync() { return Faire(ReadLine); }

        public virtual Task<int> ReadAsync(char[] buffer, int index, int count)
        {
            try { return Task.FromResult(Read(buffer, index, count)); }
            catch (Exception e) { return Task.FromException<int>(e); }
        }

        private static Task<string> Faire(Func<string> f)
        {
            try { return Task.FromResult(f()); }
            catch (Exception e) { return Task.FromException<string>(e); }
        }

        public static TextReader Synchronized(TextReader reader)
        {
            if (reader == null) throw new ArgumentNullException("reader");
            return reader;
        }

        private sealed class LecteurNul : TextReader
        {
            public override string ReadLine() { return null; }
        }
    }

    public class StringWriter : TextWriter
    {
        private static Encoding _encodage;
        private readonly StringBuilder _sb;
        private bool _ouvert = true;

        public StringWriter() : this(new StringBuilder(), null) { }
        public StringWriter(IFormatProvider formatProvider) : this(new StringBuilder(), formatProvider) { }
        public StringWriter(StringBuilder sb) : this(sb, null) { }

        public StringWriter(StringBuilder sb, IFormatProvider formatProvider) : base(formatProvider)
        {
            if (sb == null) throw new ArgumentNullException("sb");
            _sb = sb;
        }

        public override Encoding Encoding
        {
            get
            {
                if (_encodage == null) _encodage = new UnicodeEncoding(false, false);
                return _encodage;
            }
        }

        private void Ouvert() { if (!_ouvert) throw new ObjectDisposedException(null, "Cannot write to a closed TextWriter."); }

        public virtual StringBuilder GetStringBuilder() { return _sb; }

        public override void Write(char value) { Ouvert(); _sb.Append(value); }

        public override void Write(char[] buffer, int index, int count)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            Ouvert();
            _sb.Append(buffer, index, count);
        }

        public override void Write(string value)
        {
            Ouvert();
            if (value != null) _sb.Append(value);
        }

        protected override void Dispose(bool disposing) { _ouvert = false; }

        public override string ToString() { return _sb.ToString(); }
    }

    public class StringReader : TextReader
    {
        private string _texte;
        private int _position;

        public StringReader(string s)
        {
            if (s == null) throw new ArgumentNullException("s");
            _texte = s;
        }

        private string Texte
        {
            get
            {
                if (_texte == null) throw new ObjectDisposedException(null, "Cannot read from a closed TextReader.");
                return _texte;
            }
        }

        public override int Peek() { string t = Texte; return _position < t.Length ? t[_position] : -1; }
        public override int Read() { string t = Texte; return _position < t.Length ? t[_position++] : -1; }

        public override int Read(char[] buffer, int index, int count)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            if (index < 0 || count < 0) throw new ArgumentOutOfRangeException(index < 0 ? "index" : "count", "Non-negative number required.");
            if (buffer.Length - index < count) throw new ArgumentException("Offset and length were out of bounds for the array or count is greater than the number of elements from index to the end of the source collection.");
            string t = Texte;
            int n = t.Length - _position;
            if (n > count) n = count;
            if (n <= 0) return 0;
            t.CopyTo(_position, buffer, index, n);
            _position += n;
            return n;
        }

        public override string ReadToEnd()
        {
            string t = Texte;
            string r = _position == 0 ? t : t.Substring(_position);
            _position = t.Length;
            return r;
        }

        public override string ReadLine()
        {
            string t = Texte;
            if (_position >= t.Length) return null;
            int i = _position;
            while (i < t.Length && t[i] != '\r' && t[i] != '\n') i++;
            string r = t.Substring(_position, i - _position);
            if (i < t.Length && t[i] == '\r' && i + 1 < t.Length && t[i + 1] == '\n') i++;
            _position = i < t.Length ? i + 1 : i;
            return r;
        }

        protected override void Dispose(bool disposing) { _texte = null; }
    }

    public class StreamWriter : TextWriter
    {
        public static new readonly StreamWriter Null = new StreamWriter(Stream.Null, new UTF8Encoding(false, true), 128, true);

        private static Encoding _utf8SansBom;
        private Stream _flux;
        private readonly Encoding _encodage;
        private readonly bool _laisserOuvert;
        private readonly char[] _tampon;
        private int _remplis;
        private bool _preambule;

        internal static Encoding Utf8SansBom
        {
            get
            {
                if (_utf8SansBom == null) _utf8SansBom = new UTF8Encoding(false, true);
                return _utf8SansBom;
            }
        }

        public StreamWriter(Stream stream) : this(stream, Utf8SansBom, 1024, false) { }
        public StreamWriter(Stream stream, Encoding encoding) : this(stream, encoding, 1024, false) { }
        public StreamWriter(Stream stream, Encoding encoding, int bufferSize) : this(stream, encoding, bufferSize, false) { }

        public StreamWriter(Stream stream, Encoding encoding, int bufferSize, bool leaveOpen)
        {
            if (stream == null) throw new ArgumentNullException("stream");
            if (encoding == null) throw new ArgumentNullException("encoding");
            if (!stream.CanWrite) throw new ArgumentException("Stream was not writable.");
            if (bufferSize <= 0) throw new ArgumentOutOfRangeException("bufferSize", "Positive number required.");
            _flux = stream;
            _encodage = encoding;
            _laisserOuvert = leaveOpen;
            _tampon = new char[bufferSize < 128 ? 128 : bufferSize];
            // La marque d'ordre n'est écrite qu'au début du flux.
            _preambule = !stream.CanSeek || stream.Position == 0;
        }

        public StreamWriter(string path) : this(path, false, Utf8SansBom, 1024) { }
        public StreamWriter(string path, bool append) : this(path, append, Utf8SansBom, 1024) { }
        public StreamWriter(string path, bool append, Encoding encoding) : this(path, append, encoding, 1024) { }

        public StreamWriter(string path, bool append, Encoding encoding, int bufferSize)
            : this(new FileStream(path, append ? FileMode.Append : FileMode.Create, FileAccess.Write, FileShare.Read), encoding, bufferSize, false) { }

        public virtual bool AutoFlush { get; set; }
        public virtual Stream BaseStream { get { return _flux; } }
        public override Encoding Encoding { get { return _encodage; } }

        private void Ouvert() { if (_flux == null) throw new ObjectDisposedException(null, "Cannot write to a closed TextWriter."); }

        private void Vider(bool fluxAussi)
        {
            Ouvert();
            if (_preambule)
            {
                _preambule = false;
                byte[] bom = _encodage.GetPreamble();
                if (bom.Length > 0) _flux.Write(bom, 0, bom.Length);
            }
            if (_remplis > 0)
            {
                byte[] b = _encodage.GetBytes(_tampon, 0, _remplis);
                _remplis = 0;
                _flux.Write(b, 0, b.Length);
            }
            if (fluxAussi) _flux.Flush();
        }

        public override void Flush() { Vider(true); }

        public override void Write(char value)
        {
            Ouvert();
            if (_remplis == _tampon.Length) Vider(false);
            _tampon[_remplis++] = value;
            if (AutoFlush) Vider(true);
        }

        public override void Write(char[] buffer, int index, int count)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            Ouvert();
            while (count > 0)
            {
                if (_remplis == _tampon.Length) Vider(false);
                int n = _tampon.Length - _remplis;
                if (n > count) n = count;
                Array.Copy(buffer, index, _tampon, _remplis, n);
                _remplis += n;
                index += n;
                count -= n;
            }
            if (AutoFlush) Vider(true);
        }

        public override void Write(string value)
        {
            if (value == null) return;
            Write(value.ToCharArray(), 0, value.Length);
        }

        protected override void Dispose(bool disposing)
        {
            if (_flux == null) return;
            try { Vider(true); }
            finally
            {
                if (!_laisserOuvert) _flux.Close();
                _flux = null;
            }
        }
    }

    public class StreamReader : TextReader
    {
        public static new readonly StreamReader Null = new StreamReader(Stream.Null);

        private Stream _flux;
        private Encoding _encodage;
        private bool _detecter;
        private readonly bool _laisserOuvert;
        private readonly byte[] _octets;
        private int _octetsGardes;
        private char[] _car = new char[0];
        private int _pos;
        private int _fin;
        private bool _finFlux;

        public StreamReader(Stream stream) : this(stream, Encoding.UTF8, true, 1024, false) { }
        public StreamReader(Stream stream, bool detectEncodingFromByteOrderMarks) : this(stream, Encoding.UTF8, detectEncodingFromByteOrderMarks, 1024, false) { }
        public StreamReader(Stream stream, Encoding encoding) : this(stream, encoding, true, 1024, false) { }
        public StreamReader(Stream stream, Encoding encoding, bool detectEncodingFromByteOrderMarks) : this(stream, encoding, detectEncodingFromByteOrderMarks, 1024, false) { }
        public StreamReader(Stream stream, Encoding encoding, bool detectEncodingFromByteOrderMarks, int bufferSize) : this(stream, encoding, detectEncodingFromByteOrderMarks, bufferSize, false) { }

        public StreamReader(Stream stream, Encoding encoding, bool detectEncodingFromByteOrderMarks, int bufferSize, bool leaveOpen)
        {
            if (stream == null) throw new ArgumentNullException("stream");
            if (encoding == null) throw new ArgumentNullException("encoding");
            if (!stream.CanRead) throw new ArgumentException("Stream was not readable.");
            if (bufferSize <= 0) throw new ArgumentOutOfRangeException("bufferSize", "Positive number required.");
            _flux = stream;
            _encodage = encoding;
            _detecter = detectEncodingFromByteOrderMarks;
            _laisserOuvert = leaveOpen;
            _octets = new byte[bufferSize < 128 ? 128 : bufferSize];
        }

        public StreamReader(string path) : this(path, Encoding.UTF8, true, 4096) { }
        public StreamReader(string path, bool detectEncodingFromByteOrderMarks) : this(path, Encoding.UTF8, detectEncodingFromByteOrderMarks, 4096) { }
        public StreamReader(string path, Encoding encoding) : this(path, encoding, true, 4096) { }
        public StreamReader(string path, Encoding encoding, bool detectEncodingFromByteOrderMarks) : this(path, encoding, detectEncodingFromByteOrderMarks, 4096) { }

        public StreamReader(string path, Encoding encoding, bool detectEncodingFromByteOrderMarks, int bufferSize)
            : this(new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read), encoding, detectEncodingFromByteOrderMarks, bufferSize, false) { }

        public virtual Stream BaseStream { get { return _flux; } }
        public virtual Encoding CurrentEncoding { get { return _encodage; } }

        public bool EndOfStream
        {
            get { return _pos >= _fin && !Remplir(); }
        }

        public void DiscardBufferedData()
        {
            _pos = _fin = 0;
            _octetsGardes = 0;
            _finFlux = false;
        }

        // Octets complets au début de `n` octets : une séquence UTF-8 ou une
        // unité UTF-16/UTF-32 coupée attend la lecture suivante.
        private int Complets(int n)
        {
            switch (_encodage.CodePage)
            {
                case 65001:
                    for (int recul = 1; recul <= 3 && recul <= n; recul++)
                    {
                        byte b = _octets[n - recul];
                        if ((b & 0xC0) == 0x80) continue;
                        int taille = b >= 0xF0 ? 4 : b >= 0xE0 ? 3 : b >= 0xC0 ? 2 : 1;
                        return taille > recul ? n - recul : n;
                    }
                    return n;
                case 1200:
                case 1201: return n & ~1;
                case 12000:
                case 12001: return n & ~3;
                default: return n;
            }
        }

        private int Preambule(int n)
        {
            _detecter = false;
            if (n >= 3 && _octets[0] == 0xEF && _octets[1] == 0xBB && _octets[2] == 0xBF) { _encodage = Encoding.UTF8; return 3; }
            if (n >= 4 && _octets[0] == 0xFF && _octets[1] == 0xFE && _octets[2] == 0 && _octets[3] == 0) { _encodage = Encoding.UTF32; return 4; }
            if (n >= 2 && _octets[0] == 0xFF && _octets[1] == 0xFE) { _encodage = Encoding.Unicode; return 2; }
            if (n >= 2 && _octets[0] == 0xFE && _octets[1] == 0xFF) { _encodage = Encoding.BigEndianUnicode; return 2; }
            return 0;
        }

        private bool Remplir()
        {
            if (_flux == null) throw new ObjectDisposedException(null, "Cannot read from a closed TextReader.");
            while (!_finFlux)
            {
                int lus = _flux.Read(_octets, _octetsGardes, _octets.Length - _octetsGardes);
                int n = _octetsGardes + lus;
                if (lus == 0) _finFlux = true;
                int debut = 0;
                if (_detecter && (n >= 4 || _finFlux)) debut = Preambule(n);
                else if (_detecter) { _octetsGardes = n; continue; }
                else if (_encodage.CodePage == 65001 && _pos == 0 && _fin == 0 && n >= 3 && _octets[0] == 0xEF && _octets[1] == 0xBB && _octets[2] == 0xBF && _car.Length == 0)
                    debut = 3;
                int complets = _finFlux ? n : Complets(n);
                _car = _encodage.GetChars(_octets, debut, complets - debut);
                _pos = 0;
                _fin = _car.Length;
                _octetsGardes = n - complets;
                if (_octetsGardes > 0) Array.Copy(_octets, complets, _octets, 0, _octetsGardes);
                if (_fin > 0) return true;
            }
            return false;
        }

        public override int Peek()
        {
            if (_pos >= _fin && !Remplir()) return -1;
            return _car[_pos];
        }

        public override int Read()
        {
            if (_pos >= _fin && !Remplir()) return -1;
            return _car[_pos++];
        }

        public override int Read(char[] buffer, int index, int count)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            if (index < 0 || count < 0) throw new ArgumentOutOfRangeException(index < 0 ? "index" : "count", "Non-negative number required.");
            if (buffer.Length - index < count) throw new ArgumentException("Offset and length were out of bounds for the array or count is greater than the number of elements from index to the end of the source collection.");
            if (count == 0) return 0;
            if (_pos >= _fin && !Remplir()) return 0;
            int n = _fin - _pos;
            if (n > count) n = count;
            Array.Copy(_car, _pos, buffer, index, n);
            _pos += n;
            return n;
        }

        public override string ReadToEnd()
        {
            StringBuilder sb = new StringBuilder();
            while (_pos < _fin || Remplir())
            {
                sb.Append(_car, _pos, _fin - _pos);
                _pos = _fin;
            }
            return sb.ToString();
        }

        public override string ReadLine()
        {
            if (_pos >= _fin && !Remplir()) return null;
            StringBuilder sb = null;
            while (true)
            {
                int i = _pos;
                while (i < _fin && _car[i] != '\r' && _car[i] != '\n') i++;
                if (i < _fin)
                {
                    string morceau = new string(_car, _pos, i - _pos);
                    char c = _car[i];
                    _pos = i + 1;
                    if (c == '\r' && Peek() == '\n') _pos++;
                    return sb == null ? morceau : sb.Append(morceau).ToString();
                }
                if (sb == null) sb = new StringBuilder();
                sb.Append(_car, _pos, _fin - _pos);
                _pos = _fin;
                if (!Remplir()) return sb.ToString();
            }
        }

        protected override void Dispose(bool disposing)
        {
            if (_flux != null && !_laisserOuvert && disposing) _flux.Close();
            _flux = null;
        }
    }
}
