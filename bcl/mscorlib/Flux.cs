// Flux de la mscorlib Winlux (#2012) : Stream, MemoryStream, FileStream,
// BinaryReader et BinaryWriter. Licence GPL 3. Écrit d'après la
// documentation publique de .NET Framework 4.x (MS Learn : « Stream
// Class », « FileStream Class », « BinaryWriter.Write7BitEncodedInt ») ;
// aucun code de Microsoft, de Mono ou de Wine.
//
// Les méthodes « Async » se terminent avant de rendre leur tâche : le
// contrat .NET le permet, et nos fichiers n'ont pas d'entrées-sorties
// superposées.
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace System.IO
{
    public abstract class Stream : IDisposable
    {
        public static readonly Stream Null = new FluxNul();

        public abstract bool CanRead { get; }
        public abstract bool CanSeek { get; }
        public abstract bool CanWrite { get; }
        public virtual bool CanTimeout { get { return false; } }
        public abstract long Length { get; }
        public abstract long Position { get; set; }

        public virtual int ReadTimeout
        {
            get { throw new InvalidOperationException("Timeouts are not supported on this stream."); }
            set { throw new InvalidOperationException("Timeouts are not supported on this stream."); }
        }

        public virtual int WriteTimeout
        {
            get { throw new InvalidOperationException("Timeouts are not supported on this stream."); }
            set { throw new InvalidOperationException("Timeouts are not supported on this stream."); }
        }

        public abstract void Flush();
        public abstract int Read(byte[] buffer, int offset, int count);
        public abstract long Seek(long offset, SeekOrigin origin);
        public abstract void SetLength(long value);
        public abstract void Write(byte[] buffer, int offset, int count);

        public virtual int ReadByte()
        {
            byte[] un = new byte[1];
            return Read(un, 0, 1) == 0 ? -1 : un[0];
        }

        public virtual void WriteByte(byte value) { Write(new byte[] { value }, 0, 1); }

        public void CopyTo(Stream destination) { CopyTo(destination, 81920); }

        public void CopyTo(Stream destination, int bufferSize)
        {
            if (destination == null) throw new ArgumentNullException("destination");
            if (bufferSize <= 0) throw new ArgumentOutOfRangeException("bufferSize");
            byte[] tampon = new byte[bufferSize];
            int n;
            while ((n = Read(tampon, 0, tampon.Length)) > 0) destination.Write(tampon, 0, n);
        }

        public virtual void Close() { Dispose(true); }
        public void Dispose() { Close(); }
        protected virtual void Dispose(bool disposing) { }

        public Task<int> ReadAsync(byte[] buffer, int offset, int count) { return ReadAsync(buffer, offset, count, CancellationToken.None); }

        public virtual Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
        {
            try { return Task.FromResult(Read(buffer, offset, count)); }
            catch (Exception e) { return Task.FromException<int>(e); }
        }

        public Task WriteAsync(byte[] buffer, int offset, int count) { return WriteAsync(buffer, offset, count, CancellationToken.None); }

        public virtual Task WriteAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
        {
            try { Write(buffer, offset, count); return Task.CompletedTask; }
            catch (Exception e) { return Task.FromException(e); }
        }

        public Task FlushAsync() { return FlushAsync(CancellationToken.None); }

        public virtual Task FlushAsync(CancellationToken cancellationToken)
        {
            try { Flush(); return Task.CompletedTask; }
            catch (Exception e) { return Task.FromException(e); }
        }

        public Task CopyToAsync(Stream destination) { return CopyToAsync(destination, 81920, CancellationToken.None); }
        public Task CopyToAsync(Stream destination, int bufferSize) { return CopyToAsync(destination, bufferSize, CancellationToken.None); }

        public virtual Task CopyToAsync(Stream destination, int bufferSize, CancellationToken cancellationToken)
        {
            try { CopyTo(destination, bufferSize); return Task.CompletedTask; }
            catch (Exception e) { return Task.FromException(e); }
        }

        public static Stream Synchronized(Stream stream)
        {
            if (stream == null) throw new ArgumentNullException("stream");
            return stream;
        }

        internal static void VerifierTampon(byte[] buffer, int offset, int count)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            if (offset < 0) throw new ArgumentOutOfRangeException("offset", "Non-negative number required.");
            if (count < 0) throw new ArgumentOutOfRangeException("count", "Non-negative number required.");
            if (buffer.Length - offset < count) throw new ArgumentException("Offset and length were out of bounds for the array or count is greater than the number of elements from index to the end of the source collection.");
        }

        private sealed class FluxNul : Stream
        {
            public override bool CanRead { get { return true; } }
            public override bool CanSeek { get { return true; } }
            public override bool CanWrite { get { return true; } }
            public override long Length { get { return 0; } }
            public override long Position { get { return 0; } set { } }
            public override void Flush() { }
            public override int Read(byte[] buffer, int offset, int count) { return 0; }
            public override long Seek(long offset, SeekOrigin origin) { return 0; }
            public override void SetLength(long value) { }
            public override void Write(byte[] buffer, int offset, int count) { }
        }
    }

    public class MemoryStream : Stream
    {
        private byte[] _tampon;
        private readonly int _origine;
        private int _position;
        private int _longueur;
        private int _capacite;
        private readonly bool _extensible;
        private readonly bool _ecriture;
        private readonly bool _visible;
        private bool _ouvert = true;

        public MemoryStream() : this(0) { }

        public MemoryStream(int capacity)
        {
            if (capacity < 0) throw new ArgumentOutOfRangeException("capacity", "Capacity must be positive.");
            _tampon = new byte[capacity];
            _capacite = capacity;
            _extensible = true;
            _ecriture = true;
            _visible = true;
        }

        public MemoryStream(byte[] buffer) : this(buffer, true) { }

        public MemoryStream(byte[] buffer, bool writable)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            _tampon = buffer;
            _longueur = _capacite = buffer.Length;
            _ecriture = writable;
        }

        public MemoryStream(byte[] buffer, int index, int count) : this(buffer, index, count, true, false) { }
        public MemoryStream(byte[] buffer, int index, int count, bool writable) : this(buffer, index, count, writable, false) { }

        public MemoryStream(byte[] buffer, int index, int count, bool writable, bool publiclyVisible)
        {
            VerifierTampon(buffer, index, count);
            _tampon = buffer;
            _origine = _position = index;
            _longueur = _capacite = index + count;
            _ecriture = writable;
            _visible = publiclyVisible;
        }

        private void Ouvert() { if (!_ouvert) throw new ObjectDisposedException(null, "Cannot access a closed Stream."); }

        public override bool CanRead { get { return _ouvert; } }
        public override bool CanSeek { get { return _ouvert; } }
        public override bool CanWrite { get { return _ouvert && _ecriture; } }
        public override long Length { get { Ouvert(); return _longueur - _origine; } }

        public virtual int Capacity
        {
            get { Ouvert(); return _capacite - _origine; }
            set
            {
                Ouvert();
                if (value < Length) throw new ArgumentOutOfRangeException("value", "capacity was less than the current size.");
                if (!_extensible && value != Capacity) throw new NotSupportedException("Memory stream is not expandable.");
                if (_extensible && value != _capacite)
                {
                    byte[] nouveau = new byte[value];
                    if (_longueur > 0) Array.Copy(_tampon, 0, nouveau, 0, _longueur);
                    _tampon = nouveau;
                    _capacite = value;
                }
            }
        }

        public override long Position
        {
            get { Ouvert(); return _position - _origine; }
            set
            {
                Ouvert();
                if (value < 0 || value > int.MaxValue) throw new ArgumentOutOfRangeException("value", "Non-negative number required.");
                _position = _origine + (int)value;
            }
        }

        public override void Flush() { }

        public virtual byte[] GetBuffer()
        {
            if (!_visible) throw new UnauthorizedAccessException("MemoryStream's internal buffer cannot be accessed.");
            return _tampon;
        }

        public virtual byte[] ToArray()
        {
            byte[] r = new byte[_longueur - _origine];
            Array.Copy(_tampon, _origine, r, 0, r.Length);
            return r;
        }

        public override int Read(byte[] buffer, int offset, int count)
        {
            VerifierTampon(buffer, offset, count);
            Ouvert();
            int n = _longueur - _position;
            if (n > count) n = count;
            if (n <= 0) return 0;
            Array.Copy(_tampon, _position, buffer, offset, n);
            _position += n;
            return n;
        }

        public override int ReadByte()
        {
            Ouvert();
            return _position >= _longueur ? -1 : _tampon[_position++];
        }

        public override long Seek(long offset, SeekOrigin loc)
        {
            Ouvert();
            long base_;
            switch (loc)
            {
                case SeekOrigin.Begin: base_ = _origine; break;
                case SeekOrigin.Current: base_ = _position; break;
                case SeekOrigin.End: base_ = _longueur; break;
                default: throw new ArgumentException("Invalid seek origin.");
            }
            long cible = base_ + offset;
            if (cible < _origine) throw new IOException("An attempt was made to move the position before the beginning of the stream.");
            if (cible > int.MaxValue) throw new ArgumentOutOfRangeException("offset");
            _position = (int)cible;
            return _position - _origine;
        }

        private void Assurer(int fin)
        {
            if (fin <= _capacite) return;
            if (!_extensible) throw new NotSupportedException("Memory stream is not expandable.");
            int c = _capacite < 256 ? 256 : _capacite * 2;
            if (c < fin) c = fin;
            Capacity = c;
        }

        public override void SetLength(long value)
        {
            Ouvert();
            if (!_ecriture) throw new NotSupportedException("Stream does not support writing.");
            if (value < 0 || value > int.MaxValue - _origine) throw new ArgumentOutOfRangeException("value");
            int fin = _origine + (int)value;
            Assurer(fin);
            if (fin > _longueur) Array.Clear(_tampon, _longueur, fin - _longueur);
            _longueur = fin;
            if (_position > fin) _position = fin;
        }

        public override void Write(byte[] buffer, int offset, int count)
        {
            VerifierTampon(buffer, offset, count);
            Ouvert();
            if (!_ecriture) throw new NotSupportedException("Stream does not support writing.");
            int fin = _position + count;
            if (fin < 0) throw new IOException("Stream was too long.");
            Assurer(fin);
            if (_position > _longueur) Array.Clear(_tampon, _longueur, _position - _longueur);
            Array.Copy(buffer, offset, _tampon, _position, count);
            _position = fin;
            if (fin > _longueur) _longueur = fin;
        }

        public override void WriteByte(byte value)
        {
            Ouvert();
            if (!_ecriture) throw new NotSupportedException("Stream does not support writing.");
            Assurer(_position + 1);
            if (_position > _longueur) Array.Clear(_tampon, _longueur, _position - _longueur);
            _tampon[_position++] = value;
            if (_position > _longueur) _longueur = _position;
        }

        public virtual void WriteTo(Stream stream)
        {
            if (stream == null) throw new ArgumentNullException("stream");
            Ouvert();
            stream.Write(_tampon, _origine, _longueur - _origine);
        }

        protected override void Dispose(bool disposing) { _ouvert = false; }
    }

    // FileStream sur un handle de kernel32. Les écritures passent par un
    // tampon de 4 Kio, vidé avant toute lecture, tout déplacement et à la
    // fermeture.
    public class FileStream : Stream
    {
        private long _h = -1;
        private readonly string _nom;
        private readonly FileAccess _acces;
        private readonly bool _ajout;
        private long _debutAjout;
        private byte[] _tampon;
        private int _remplis;

        public FileStream(string path, FileMode mode)
            : this(path, mode, mode == FileMode.Append ? FileAccess.Write : FileAccess.ReadWrite, FileShare.Read, 4096, FileOptions.None) { }

        public FileStream(string path, FileMode mode, FileAccess access)
            : this(path, mode, access, FileShare.Read, 4096, FileOptions.None) { }

        public FileStream(string path, FileMode mode, FileAccess access, FileShare share)
            : this(path, mode, access, share, 4096, FileOptions.None) { }

        public FileStream(string path, FileMode mode, FileAccess access, FileShare share, int bufferSize)
            : this(path, mode, access, share, bufferSize, FileOptions.None) { }

        public FileStream(string path, FileMode mode, FileAccess access, FileShare share, int bufferSize, bool useAsync)
            : this(path, mode, access, share, bufferSize, useAsync ? FileOptions.Asynchronous : FileOptions.None) { }

        public FileStream(string path, FileMode mode, FileAccess access, FileShare share, int bufferSize, FileOptions options)
        {
            if (path == null) throw new ArgumentNullException("path");
            if (path.Length == 0) throw new ArgumentException("Empty path name is not legal.", "path");
            if (mode < FileMode.CreateNew || mode > FileMode.Append) throw new ArgumentOutOfRangeException("mode", "Enum value was out of legal range.");
            if (access < FileAccess.Read || access > FileAccess.ReadWrite) throw new ArgumentOutOfRangeException("access", "Enum value was out of legal range.");
            if (bufferSize <= 0) throw new ArgumentOutOfRangeException("bufferSize", "Positive number required.");
            if ((access & FileAccess.Write) == 0 && (mode == FileMode.Truncate || mode == FileMode.CreateNew || mode == FileMode.Create || mode == FileMode.Append))
                throw new ArgumentException("Combining FileMode: " + mode + " with FileAccess: " + access + " is invalid.", "access");
            if (mode == FileMode.Append && (access & FileAccess.Read) != 0)
                throw new ArgumentException("Append access can be requested only in write-only mode.", "access");

            int droits = 0;
            if ((access & FileAccess.Read) != 0) droits |= unchecked((int)0x80000000);
            if ((access & FileAccess.Write) != 0) droits |= 0x40000000;
            int disposition = mode == FileMode.Append ? 4 : (int)mode;
            int drapeaux = ((int)options & ~(int)FileOptions.Asynchronous) | 0x80;

            long h = Natif.Ouvrir(path, disposition, droits, (int)(share & ~FileShare.Inheritable), drapeaux);
            if (h < 0) throw Natif.Erreur((int)h, path);
            _h = h;
            _nom = Path.GetFullPath(path);
            _acces = access;
            if (mode == FileMode.Append)
            {
                _ajout = true;
                _debutAjout = Natif.Positionner(_h, 0, 2);
            }
        }

        public virtual string Name { get { return _nom ?? "[Unknown]"; } }
        public virtual bool IsAsync { get { return false; } }

        public override bool CanRead { get { return _h >= 0 && (_acces & FileAccess.Read) != 0; } }
        public override bool CanWrite { get { return _h >= 0 && (_acces & FileAccess.Write) != 0; } }
        public override bool CanSeek { get { return _h >= 0; } }

        private void Ouvert() { if (_h < 0) throw new ObjectDisposedException(null, "Cannot access a closed file."); }

        private void Vider()
        {
            if (_remplis == 0) return;
            int fait = 0;
            while (fait < _remplis)
            {
                int n = Natif.Ecrire(_h, _tampon, fait, _remplis - fait);
                if (n < 0) { _remplis = 0; throw Natif.Erreur(n, _nom); }
                fait += n;
            }
            _remplis = 0;
        }

        public override long Length
        {
            get
            {
                Ouvert();
                Vider();
                long t = Natif.Taille(_h);
                if (t < 0) throw Natif.Erreur((int)t, _nom);
                return t;
            }
        }

        public override long Position
        {
            get
            {
                Ouvert();
                long p = Natif.Positionner(_h, 0, 1);
                if (p < 0) throw Natif.Erreur((int)p, _nom);
                return p + _remplis;
            }
            set
            {
                if (value < 0) throw new ArgumentOutOfRangeException("value", "Non-negative number required.");
                Seek(value, SeekOrigin.Begin);
            }
        }

        public override void Flush() { Ouvert(); Vider(); }

        public virtual void Flush(bool flushToDisk)
        {
            Flush();
            if (flushToDisk) Natif.Vider(_h);
        }

        public override int Read(byte[] array, int offset, int count)
        {
            VerifierTampon(array, offset, count);
            Ouvert();
            if ((_acces & FileAccess.Read) == 0) throw new NotSupportedException("Stream does not support reading.");
            Vider();
            if (count == 0) return 0;
            int n = Natif.Lire(_h, array, offset, count);
            if (n < 0) throw Natif.Erreur(n, _nom);
            return n;
        }

        public override long Seek(long offset, SeekOrigin origin)
        {
            Ouvert();
            if (origin < SeekOrigin.Begin || origin > SeekOrigin.End) throw new ArgumentException("Invalid seek origin.");
            Vider();
            long avant = Natif.Positionner(_h, 0, 1);
            long p = Natif.Positionner(_h, offset, (int)origin);
            if (p < 0) throw p == -131 ? new IOException("An attempt was made to move the position before the beginning of the stream.") : Natif.Erreur((int)p, _nom);
            if (_ajout && p < _debutAjout)
            {
                Natif.Positionner(_h, avant, 0);
                throw new IOException("Unable seek backward to overwrite data that previously existed in a file opened in Append mode.");
            }
            return p;
        }

        public override void SetLength(long value)
        {
            if (value < 0) throw new ArgumentOutOfRangeException("value", "Non-negative number required.");
            Ouvert();
            if ((_acces & FileAccess.Write) == 0) throw new NotSupportedException("Stream does not support writing.");
            Vider();
            Natif.Verifier(Natif.FixerTaille(_h, value), _nom);
        }

        public override void Write(byte[] array, int offset, int count)
        {
            VerifierTampon(array, offset, count);
            Ouvert();
            if ((_acces & FileAccess.Write) == 0) throw new NotSupportedException("Stream does not support writing.");
            if (_tampon == null) _tampon = new byte[4096];
            if (_remplis + count <= _tampon.Length)
            {
                Array.Copy(array, offset, _tampon, _remplis, count);
                _remplis += count;
                return;
            }
            Vider();
            int fait = 0;
            while (fait < count)
            {
                int n = Natif.Ecrire(_h, array, offset + fait, count - fait);
                if (n < 0) throw Natif.Erreur(n, _nom);
                fait += n;
            }
        }

        public override void WriteByte(byte value)
        {
            Ouvert();
            if ((_acces & FileAccess.Write) == 0) throw new NotSupportedException("Stream does not support writing.");
            if (_tampon == null) _tampon = new byte[4096];
            if (_remplis == _tampon.Length) Vider();
            _tampon[_remplis++] = value;
        }

        protected override void Dispose(bool disposing)
        {
            if (_h < 0) return;
            try { Vider(); }
            finally
            {
                Natif.Fermer(_h);
                _h = -1;
            }
        }
    }

    public class BinaryWriter : IDisposable
    {
        public static readonly BinaryWriter Null = new BinaryWriter();

        protected Stream OutStream;
        private readonly Encoding _encodage;
        private readonly bool _laisserOuvert;
        private readonly byte[] _tampon = new byte[16];

        protected BinaryWriter() { OutStream = Stream.Null; _encodage = new UTF8Encoding(false, true); }

        public BinaryWriter(Stream output) : this(output, new UTF8Encoding(false, true), false) { }
        public BinaryWriter(Stream output, Encoding encoding) : this(output, encoding, false) { }

        public BinaryWriter(Stream output, Encoding encoding, bool leaveOpen)
        {
            if (output == null) throw new ArgumentNullException("output");
            if (encoding == null) throw new ArgumentNullException("encoding");
            if (!output.CanWrite) throw new ArgumentException("Stream was not writable.");
            OutStream = output;
            _encodage = encoding;
            _laisserOuvert = leaveOpen;
        }

        public virtual Stream BaseStream { get { Flush(); return OutStream; } }

        public virtual void Close() { Dispose(true); }
        public void Dispose() { Dispose(true); }

        protected virtual void Dispose(bool disposing)
        {
            if (!disposing) return;
            if (_laisserOuvert) OutStream.Flush();
            else OutStream.Close();
        }

        public virtual void Flush() { OutStream.Flush(); }
        public virtual long Seek(int offset, SeekOrigin origin) { return OutStream.Seek(offset, origin); }

        private void Octets(ulong v, int n)
        {
            for (int i = 0; i < n; i++) { _tampon[i] = (byte)v; v >>= 8; }
            OutStream.Write(_tampon, 0, n);
        }

        public virtual void Write(bool value) { OutStream.WriteByte(value ? (byte)1 : (byte)0); }
        public virtual void Write(byte value) { OutStream.WriteByte(value); }
        public virtual void Write(sbyte value) { OutStream.WriteByte((byte)value); }

        public virtual void Write(byte[] buffer)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            OutStream.Write(buffer, 0, buffer.Length);
        }

        public virtual void Write(byte[] buffer, int index, int count) { OutStream.Write(buffer, index, count); }

        public virtual void Write(char ch)
        {
            if (char.IsSurrogate(ch)) throw new ArgumentException("Surrogate characters are not allowed in this context.");
            Write(new char[] { ch });
        }

        public virtual void Write(char[] chars)
        {
            if (chars == null) throw new ArgumentNullException("chars");
            Write(chars, 0, chars.Length);
        }

        public virtual void Write(char[] chars, int index, int count)
        {
            byte[] b = _encodage.GetBytes(chars, index, count);
            OutStream.Write(b, 0, b.Length);
        }

        public virtual void Write(short value) { Octets((ulong)value, 2); }
        public virtual void Write(ushort value) { Octets(value, 2); }
        public virtual void Write(int value) { Octets((ulong)value, 4); }
        public virtual void Write(uint value) { Octets(value, 4); }
        public virtual void Write(long value) { Octets((ulong)value, 8); }
        public virtual void Write(ulong value) { Octets(value, 8); }
        public virtual void Write(float value) { Octets((uint)BitConverter.SingleToInt32Bits(value), 4); }
        public virtual void Write(double value) { Octets((ulong)BitConverter.DoubleToInt64Bits(value), 8); }

        // Chaîne précédée de sa longueur en octets, sur 7 bits par octet.
        public virtual void Write(string value)
        {
            if (value == null) throw new ArgumentNullException("value");
            byte[] b = _encodage.GetBytes(value);
            Write7BitEncodedInt(b.Length);
            OutStream.Write(b, 0, b.Length);
        }

        protected void Write7BitEncodedInt(int value)
        {
            uint v = (uint)value;
            while (v >= 0x80)
            {
                OutStream.WriteByte((byte)(v | 0x80));
                v >>= 7;
            }
            OutStream.WriteByte((byte)v);
        }
    }

    public class BinaryReader : IDisposable
    {
        private Stream _flux;
        private readonly Encoding _encodage;
        private readonly bool _laisserOuvert;
        private readonly byte[] _tampon = new byte[16];

        public BinaryReader(Stream input) : this(input, new UTF8Encoding(), false) { }
        public BinaryReader(Stream input, Encoding encoding) : this(input, encoding, false) { }

        public BinaryReader(Stream input, Encoding encoding, bool leaveOpen)
        {
            if (input == null) throw new ArgumentNullException("input");
            if (encoding == null) throw new ArgumentNullException("encoding");
            if (!input.CanRead) throw new ArgumentException("Stream was not readable.");
            _flux = input;
            _encodage = encoding;
            _laisserOuvert = leaveOpen;
        }

        public virtual Stream BaseStream { get { return _flux; } }

        public virtual void Close() { Dispose(true); }
        public void Dispose() { Dispose(true); }

        protected virtual void Dispose(bool disposing)
        {
            if (disposing && _flux != null && !_laisserOuvert) _flux.Close();
            _flux = null;
        }

        private Stream Flux
        {
            get
            {
                if (_flux == null) throw new ObjectDisposedException(null, "Cannot access a closed file.");
                return _flux;
            }
        }

        protected virtual void FillBuffer(int numBytes)
        {
            int lus = 0;
            while (lus < numBytes)
            {
                int n = Flux.Read(_tampon, lus, numBytes - lus);
                if (n == 0) throw new EndOfStreamException();
                lus += n;
            }
        }

        private ulong Entier(int n)
        {
            FillBuffer(n);
            ulong v = 0;
            for (int i = n - 1; i >= 0; i--) v = (v << 8) | _tampon[i];
            return v;
        }

        public virtual bool ReadBoolean() { FillBuffer(1); return _tampon[0] != 0; }
        public virtual byte ReadByte() { int b = Flux.ReadByte(); if (b < 0) throw new EndOfStreamException(); return (byte)b; }
        public virtual sbyte ReadSByte() { return (sbyte)ReadByte(); }
        public virtual short ReadInt16() { return (short)Entier(2); }
        public virtual ushort ReadUInt16() { return (ushort)Entier(2); }
        public virtual int ReadInt32() { return (int)Entier(4); }
        public virtual uint ReadUInt32() { return (uint)Entier(4); }
        public virtual long ReadInt64() { return (long)Entier(8); }
        public virtual ulong ReadUInt64() { return Entier(8); }
        public virtual float ReadSingle() { return BitConverter.Int32BitsToSingle((int)Entier(4)); }
        public virtual double ReadDouble() { return BitConverter.Int64BitsToDouble((long)Entier(8)); }

        public virtual byte[] ReadBytes(int count)
        {
            if (count < 0) throw new ArgumentOutOfRangeException("count", "Non-negative number required.");
            byte[] r = new byte[count];
            int lus = 0;
            while (lus < count)
            {
                int n = Flux.Read(r, lus, count - lus);
                if (n == 0) break;
                lus += n;
            }
            if (lus < count) Array.Resize(ref r, lus);
            return r;
        }

        public virtual int Read(byte[] buffer, int index, int count) { return Flux.Read(buffer, index, count); }

        // Nombre d'octets du caractère qui commence par `premier`.
        private int TailleCaractere(int premier)
        {
            switch (_encodage.CodePage)
            {
                case 65001:
                    if (premier < 0x80) return 1;
                    if (premier >= 0xF0) return 4;
                    if (premier >= 0xE0) return 3;
                    return 2;
                case 1200:
                case 1201: return 2;
                case 12000:
                case 12001: return 4;
                default: return 1;
            }
        }

        public virtual int Read()
        {
            int premier = Flux.ReadByte();
            if (premier < 0) return -1;
            int n = TailleCaractere(premier);
            byte[] b = new byte[n];
            b[0] = (byte)premier;
            for (int i = 1; i < n; i++)
            {
                int x = Flux.ReadByte();
                if (x < 0) throw new EndOfStreamException();
                b[i] = (byte)x;
            }
            char[] c = _encodage.GetChars(b);
            return c.Length == 0 ? -1 : c[0];
        }

        public virtual int PeekChar()
        {
            if (!Flux.CanSeek) return -1;
            long p = Flux.Position;
            int c = Read();
            Flux.Position = p;
            return c;
        }

        public virtual char ReadChar()
        {
            int c = Read();
            if (c < 0) throw new EndOfStreamException();
            return (char)c;
        }

        public virtual char[] ReadChars(int count)
        {
            if (count < 0) throw new ArgumentOutOfRangeException("count", "Non-negative number required.");
            char[] r = new char[count];
            int n = 0;
            while (n < count)
            {
                int c = Read();
                if (c < 0) break;
                r[n++] = (char)c;
            }
            if (n < count) Array.Resize(ref r, n);
            return r;
        }

        public virtual string ReadString()
        {
            int longueur = Read7BitEncodedInt();
            if (longueur < 0) throw new IOException("BinaryReader encountered an invalid string length of " + longueur + " characters.");
            if (longueur == 0) return "";
            byte[] b = ReadBytes(longueur);
            if (b.Length < longueur) throw new EndOfStreamException();
            return _encodage.GetString(b);
        }

        protected internal int Read7BitEncodedInt()
        {
            int v = 0, decalage = 0;
            while (true)
            {
                if (decalage == 35) throw new FormatException("Too many bytes in what should have been a 7 bit encoded Int32.");
                byte b = ReadByte();
                v |= (b & 0x7F) << decalage;
                decalage += 7;
                if ((b & 0x80) == 0) return v;
            }
        }
    }
}
