// System.IO.Compression de la mscorlib Winlux (#2819, suivi #2752) :
// DeflateStream en décompression et ZipArchive en lecture, pour le paquet que
// télécharge l'installateur web de Paint.NET.
// Licence GPL 3. Écrit d'après la documentation publique : Learn
// (« ZipArchive Class », « ZipArchiveEntry Class », « ZipArchiveMode »,
// « DeflateStream Class », « InvalidDataException »), la RFC 1951 (format
// DEFLATE) et la spécification APPNOTE.TXT de PKWARE (format ZIP, Zip64).
// Aucun code de Microsoft, de Mono, de Wine ni de zlib.
//
// Limites, nommées : pas de compression (Compress et les modes Create/Update
// lèvent NotSupportedException) ; DeflateStream décompresse tout son flux
// source au premier Read ; pas de contrôle du CRC-32, comme .NET Framework 4.x.
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Text;

namespace System.IO
{
    public sealed class InvalidDataException : SystemException
    {
        public InvalidDataException() : base("Found invalid data while decoding.") { }
        public InvalidDataException(string message) : base(message) { }
        public InvalidDataException(string message, Exception innerException) : base(message, innerException) { }
    }
}

namespace System.IO.Compression
{
    public enum CompressionMode { Decompress = 0, Compress = 1 }

    public enum CompressionLevel { Optimal = 0, Fastest = 1, NoCompression = 2 }

    public enum ZipArchiveMode { Read = 0, Create = 1, Update = 2 }

    // ── Décodeur DEFLATE (RFC 1951) ──

    internal sealed class Inflateur
    {
        private readonly byte[] _entree;
        private int _pos, _bits, _nbBits;
        private readonly MemoryStream _sortie = new MemoryStream();
        private byte[] _fenetre = new byte[32768];
        private int _fen; // octets écrits en tout (la fenêtre est circulaire)

        // RFC 1951 § 3.2.5 : bases et bits supplémentaires des longueurs et distances.
        private static readonly int[] BaseLong = { 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258 };
        private static readonly int[] ExtraLong = { 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0 };
        private static readonly int[] BaseDist = { 1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577 };
        private static readonly int[] ExtraDist = { 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13 };
        // § 3.2.7 : ordre des longueurs de codes du code des longueurs.
        private static readonly int[] OrdreLongueurs = { 16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15 };

        private Inflateur(byte[] entree) { _entree = entree; }

        public static byte[] Decompresser(byte[] entree)
        {
            Inflateur i = new Inflateur(entree);
            i.Blocs();
            return i._sortie.ToArray();
        }

        // Code de Huffman canonique (§ 3.2.2) : nombre de codes par longueur
        // et symboles rangés par code croissant.
        private sealed class Huffman
        {
            public readonly int[] Nombre = new int[16];
            public readonly int[] Symboles;

            public Huffman(int[] longueurs, int n)
            {
                Symboles = new int[n];
                for (int s = 0; s < n; s++) Nombre[longueurs[s]]++;
                Nombre[0] = 0;
                int[] debut = new int[16];
                for (int l = 1; l < 16; l++) debut[l] = debut[l - 1] + Nombre[l - 1];
                for (int s = 0; s < n; s++)
                    if (longueurs[s] != 0) Symboles[debut[longueurs[s]]++] = s;
            }
        }

        private static InvalidDataException Invalide(string quoi)
        {
            return new InvalidDataException("The archive entry was compressed using an unsupported compression method or is corrupt (" + quoi + ").");
        }

        private int Bits(int n)
        {
            while (_nbBits < n)
            {
                if (_pos >= _entree.Length) throw Invalide("fin de flux");
                _bits |= _entree[_pos++] << _nbBits;
                _nbBits += 8;
            }
            int v = _bits & ((1 << n) - 1);
            _bits >>= n;
            _nbBits -= n;
            return v;
        }

        // Les codes de Huffman se lisent bit à bit, poids fort du code en premier.
        private int Decoder(Huffman h)
        {
            int code = 0, premier = 0, index = 0;
            for (int l = 1; l < 16; l++)
            {
                code |= Bits(1);
                int nombre = h.Nombre[l];
                if (code - nombre < premier) return h.Symboles[index + (code - premier)];
                index += nombre;
                premier += nombre;
                premier <<= 1;
                code <<= 1;
            }
            throw Invalide("code de Huffman");
        }

        private void Emettre(byte b)
        {
            _sortie.WriteByte(b);
            _fenetre[_fen & 32767] = b;
            _fen++;
        }

        private void Blocs()
        {
            bool dernier;
            do
            {
                dernier = Bits(1) == 1;
                int type = Bits(2);
                if (type == 0) Stocke();
                else if (type == 1) Compresse(FixeLitteraux(), FixeDistances());
                else if (type == 2) Dynamique();
                else throw Invalide("bloc de type 3");
            } while (!dernier);
        }

        private void Stocke()
        {
            _bits = 0;
            _nbBits = 0; // § 3.2.4 : on saute au prochain octet
            if (_pos + 4 > _entree.Length) throw Invalide("bloc stocké");
            int len = _entree[_pos] | (_entree[_pos + 1] << 8);
            int nlen = _entree[_pos + 2] | (_entree[_pos + 3] << 8);
            _pos += 4;
            if ((len ^ 0xFFFF) != nlen) throw Invalide("LEN/NLEN");
            if (_pos + len > _entree.Length) throw Invalide("bloc stocké tronqué");
            for (int i = 0; i < len; i++) Emettre(_entree[_pos + i]);
            _pos += len;
        }

        private static Huffman s_fixeL, s_fixeD;

        private static Huffman FixeLitteraux()
        {
            if (s_fixeL == null)
            {
                int[] l = new int[288];
                for (int i = 0; i < 144; i++) l[i] = 8;
                for (int i = 144; i < 256; i++) l[i] = 9;
                for (int i = 256; i < 280; i++) l[i] = 7;
                for (int i = 280; i < 288; i++) l[i] = 8;
                s_fixeL = new Huffman(l, 288);
            }
            return s_fixeL;
        }

        private static Huffman FixeDistances()
        {
            if (s_fixeD == null)
            {
                int[] l = new int[30];
                for (int i = 0; i < 30; i++) l[i] = 5;
                s_fixeD = new Huffman(l, 30);
            }
            return s_fixeD;
        }

        private void Dynamique()
        {
            int hlit = Bits(5) + 257, hdist = Bits(5) + 1, hclen = Bits(4) + 4;
            if (hlit > 286 || hdist > 30) throw Invalide("HLIT/HDIST");
            int[] lc = new int[19];
            for (int i = 0; i < hclen; i++) lc[OrdreLongueurs[i]] = Bits(3);
            Huffman codeLong = new Huffman(lc, 19);
            int[] longueurs = new int[hlit + hdist];
            int n = 0;
            while (n < hlit + hdist)
            {
                int s = Decoder(codeLong);
                if (s < 16) { longueurs[n++] = s; continue; }
                int rep, val = 0;
                if (s == 16)
                {
                    if (n == 0) throw Invalide("répétition sans longueur");
                    val = longueurs[n - 1];
                    rep = 3 + Bits(2);
                }
                else if (s == 17) rep = 3 + Bits(3);
                else rep = 11 + Bits(7);
                if (n + rep > hlit + hdist) throw Invalide("répétition trop longue");
                while (rep-- > 0) longueurs[n++] = val;
            }
            if (longueurs[256] == 0) throw Invalide("pas de code de fin de bloc");
            int[] ll = new int[hlit];
            int[] ld = new int[hdist];
            Array.Copy(longueurs, 0, ll, 0, hlit);
            Array.Copy(longueurs, hlit, ld, 0, hdist);
            Compresse(new Huffman(ll, hlit), new Huffman(ld, hdist));
        }

        private void Compresse(Huffman lit, Huffman dist)
        {
            while (true)
            {
                int s = Decoder(lit);
                if (s < 256) { Emettre((byte)s); continue; }
                if (s == 256) return;
                s -= 257;
                if (s >= 29) throw Invalide("longueur");
                int longueur = BaseLong[s] + Bits(ExtraLong[s]);
                int d = Decoder(dist);
                if (d >= 30) throw Invalide("distance");
                int distance = BaseDist[d] + Bits(ExtraDist[d]);
                if (distance > _fen) throw Invalide("distance avant le début");
                for (int i = 0; i < longueur; i++) Emettre(_fenetre[(_fen - distance) & 32767]);
            }
        }
    }

    public class DeflateStream : Stream
    {
        private Stream _source;
        private readonly bool _laisserOuvert;
        private MemoryStream _clair;

        public DeflateStream(Stream stream, CompressionMode mode) : this(stream, mode, false) { }

        public DeflateStream(Stream stream, CompressionMode mode, bool leaveOpen)
        {
            if (stream == null) throw new ArgumentNullException("stream");
            if (mode != CompressionMode.Decompress)
                throw new NotSupportedException("Winlux: DeflateStream compression is not implemented.");
            if (!stream.CanRead) throw new ArgumentException("Stream does not support reading.", "stream");
            _source = stream;
            _laisserOuvert = leaveOpen;
        }

        public DeflateStream(Stream stream, CompressionLevel compressionLevel) : this(stream, CompressionMode.Compress, false) { }
        public DeflateStream(Stream stream, CompressionLevel compressionLevel, bool leaveOpen) : this(stream, CompressionMode.Compress, leaveOpen) { }

        public Stream BaseStream { get { return _source; } }
        public override bool CanRead { get { return _source != null; } }
        public override bool CanSeek { get { return false; } }
        public override bool CanWrite { get { return false; } }
        public override long Length { get { throw new NotSupportedException("This operation is not supported."); } }
        public override long Position
        {
            get { throw new NotSupportedException("This operation is not supported."); }
            set { throw new NotSupportedException("This operation is not supported."); }
        }

        public override void Flush() { }
        public override long Seek(long offset, SeekOrigin origin) { throw new NotSupportedException("This operation is not supported."); }
        public override void SetLength(long value) { throw new NotSupportedException("This operation is not supported."); }
        public override void Write(byte[] buffer, int offset, int count) { throw new NotSupportedException("This operation is not supported."); }

        public override int Read(byte[] buffer, int offset, int count)
        {
            if (_source == null) throw new ObjectDisposedException("DeflateStream");
            if (buffer == null) throw new ArgumentNullException("buffer");
            if (offset < 0 || count < 0 || offset + count > buffer.Length) throw new ArgumentOutOfRangeException("count");
            if (_clair == null)
            {
                MemoryStream brut = new MemoryStream();
                _source.CopyTo(brut);
                _clair = new MemoryStream(Inflateur.Decompresser(brut.ToArray()));
            }
            return _clair.Read(buffer, offset, count);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing && _source != null && !_laisserOuvert) _source.Dispose();
            _source = null;
            base.Dispose(disposing);
        }
    }

    // Fenêtre en lecture seule sur [debut, debut + longueur) d'un flux cherchable.
    internal sealed class SousFlux : Stream
    {
        private readonly Stream _base;
        private readonly long _debut, _longueur;
        private long _pos;

        public SousFlux(Stream b, long debut, long longueur) { _base = b; _debut = debut; _longueur = longueur; }

        public override bool CanRead { get { return true; } }
        public override bool CanSeek { get { return true; } }
        public override bool CanWrite { get { return false; } }
        public override long Length { get { return _longueur; } }
        public override long Position { get { return _pos; } set { _pos = value; } }
        public override void Flush() { }
        public override void SetLength(long value) { throw new NotSupportedException(); }
        public override void Write(byte[] buffer, int offset, int count) { throw new NotSupportedException(); }

        public override long Seek(long offset, SeekOrigin origin)
        {
            long p = origin == SeekOrigin.Begin ? offset : origin == SeekOrigin.Current ? _pos + offset : _longueur + offset;
            if (p < 0) throw new IOException("An attempt was made to move the position before the beginning of the stream.");
            _pos = p;
            return p;
        }

        public override int Read(byte[] buffer, int offset, int count)
        {
            long reste = _longueur - _pos;
            if (reste <= 0) return 0;
            if (count > reste) count = (int)reste;
            _base.Position = _debut + _pos;
            int n = _base.Read(buffer, offset, count);
            _pos += n;
            return n;
        }
    }

    public class ZipArchive : IDisposable
    {
        private Stream _flux;
        private readonly bool _laisserOuvert;
        private readonly List<ZipArchiveEntry> _entrees = new List<ZipArchiveEntry>();
        private readonly ReadOnlyCollection<ZipArchiveEntry> _lecture;
        private readonly Encoding _encodage;

        public ZipArchive(Stream stream) : this(stream, ZipArchiveMode.Read, false, null) { }
        public ZipArchive(Stream stream, ZipArchiveMode mode) : this(stream, mode, false, null) { }
        public ZipArchive(Stream stream, ZipArchiveMode mode, bool leaveOpen) : this(stream, mode, leaveOpen, null) { }

        public ZipArchive(Stream stream, ZipArchiveMode mode, bool leaveOpen, Encoding entryNameEncoding)
        {
            if (stream == null) throw new ArgumentNullException("stream");
            if (mode != ZipArchiveMode.Read)
                throw new NotSupportedException("Winlux: ZipArchive supports only ZipArchiveMode.Read.");
            if (!stream.CanRead) throw new ArgumentException("Cannot use read mode on a non-readable stream.");
            _laisserOuvert = leaveOpen;
            _encodage = entryNameEncoding;
            _flux = stream;
            // Learn : un flux qui ne se cherche pas est d'abord copié en mémoire.
            if (!stream.CanSeek)
            {
                MemoryStream m = new MemoryStream();
                stream.CopyTo(m);
                m.Position = 0;
                _flux = m;
            }
            _lecture = new ReadOnlyCollection<ZipArchiveEntry>(_entrees);
            try { LireRepertoire(); }
            catch (EndOfStreamException e) { throw new InvalidDataException("Central Directory corrupt.", e); }
        }

        public ZipArchiveMode Mode { get { return ZipArchiveMode.Read; } }

        public ReadOnlyCollection<ZipArchiveEntry> Entries
        {
            get
            {
                if (_flux == null) throw new ObjectDisposedException("ZipArchive");
                return _lecture;
            }
        }

        public ZipArchiveEntry GetEntry(string entryName)
        {
            if (entryName == null) throw new ArgumentNullException("entryName");
            if (_flux == null) throw new ObjectDisposedException("ZipArchive");
            foreach (ZipArchiveEntry e in _entrees) if (e.FullName == entryName) return e;
            return null;
        }

        public ZipArchiveEntry CreateEntry(string entryName)
        {
            throw new NotSupportedException("Cannot modify read-only archive.");
        }

        public void Dispose() { Dispose(true); }

        protected virtual void Dispose(bool disposing)
        {
            if (disposing && _flux != null && !_laisserOuvert) _flux.Dispose();
            _flux = null;
        }

        internal Stream Flux
        {
            get
            {
                if (_flux == null) throw new ObjectDisposedException("ZipArchive");
                return _flux;
            }
        }

        private static uint U32(byte[] b, int i) { return (uint)(b[i] | (b[i + 1] << 8) | (b[i + 2] << 16) | (b[i + 3] << 24)); }
        private static int U16(byte[] b, int i) { return b[i] | (b[i + 1] << 8); }
        private static long U64(byte[] b, int i) { return (long)U32(b, i) | ((long)U32(b, i + 4) << 32); }

        internal void LireExact(long position, byte[] tampon)
        {
            _flux.Position = position;
            int lu = 0;
            while (lu < tampon.Length)
            {
                int n = _flux.Read(tampon, lu, tampon.Length - lu);
                if (n <= 0) throw new EndOfStreamException();
                lu += n;
            }
        }

        private void LireRepertoire()
        {
            long taille = _flux.Length;
            // APPNOTE 4.3.16 : fin du répertoire central (22 octets + commentaire ≤ 65535).
            int fenetre = (int)Math.Min(taille, 22 + 65535);
            if (fenetre < 22) throw new InvalidDataException("End of Central Directory record could not be found.");
            byte[] fin = new byte[fenetre];
            LireExact(taille - fenetre, fin);
            int eocd = -1;
            for (int i = fenetre - 22; i >= 0; i--)
                if (U32(fin, i) == 0x06054b50) { eocd = i; break; }
            if (eocd < 0) throw new InvalidDataException("End of Central Directory record could not be found.");
            long nombre = U16(fin, eocd + 10);
            long tailleRep = U32(fin, eocd + 12);
            long debutRep = U32(fin, eocd + 16);
            // Zip64 (4.3.14, 4.3.15) : le localisateur précède la fin classique.
            long posEocd = taille - fenetre + eocd;
            if ((nombre == 0xFFFF || debutRep == 0xFFFFFFFF || tailleRep == 0xFFFFFFFF) && posEocd >= 20)
            {
                byte[] loc = new byte[20];
                LireExact(posEocd - 20, loc);
                if (U32(loc, 0) == 0x07064b50)
                {
                    byte[] e64 = new byte[56];
                    LireExact(U64(loc, 8), e64);
                    if (U32(e64, 0) != 0x06064b50) throw new InvalidDataException("Zip64 End of Central Directory record not found.");
                    nombre = U64(e64, 32);
                    tailleRep = U64(e64, 40);
                    debutRep = U64(e64, 48);
                }
            }
            if (debutRep + tailleRep > taille || tailleRep > int.MaxValue) throw new InvalidDataException("Central Directory corrupt.");
            byte[] rep = new byte[tailleRep];
            LireExact(debutRep, rep);
            int p = 0;
            for (long k = 0; k < nombre; k++)
            {
                // 4.3.12 : en-tête du répertoire central, 46 octets fixes.
                if (p + 46 > rep.Length || U32(rep, p) != 0x02014b50) throw new InvalidDataException("Central Directory corrupt.");
                int drapeaux = U16(rep, p + 8);
                int methode = U16(rep, p + 10);
                int heureDos = U16(rep, p + 12), dateDos = U16(rep, p + 14);
                uint crc = U32(rep, p + 16);
                long compresse = U32(rep, p + 20), clair = U32(rep, p + 24);
                int lnom = U16(rep, p + 28), lextra = U16(rep, p + 30), lcomm = U16(rep, p + 32);
                long local = U32(rep, p + 42);
                if (p + 46 + lnom + lextra + lcomm > rep.Length) throw new InvalidDataException("Central Directory corrupt.");
                // 4.4.4 bit 11 : nom en UTF-8 ; sinon le codage donné, ou la page
                // de code par défaut du système (Learn, constructeur de ZipArchive).
                Encoding enc = (drapeaux & 0x800) != 0 ? new UTF8Encoding(false) : (_encodage ?? Encoding.Default);
                string nom = enc.GetString(rep, p + 46, lnom);
                // 4.5.3 : champ Zip64, présent pour les valeurs à 0xFFFFFFFF, dans cet ordre.
                int x = p + 46 + lnom, finExtra = x + lextra;
                while (x + 4 <= finExtra)
                {
                    int id = U16(rep, x), lg = U16(rep, x + 2);
                    if (id == 1)
                    {
                        int q = x + 4;
                        if (clair == 0xFFFFFFFF && q + 8 <= x + 4 + lg) { clair = U64(rep, q); q += 8; }
                        if (compresse == 0xFFFFFFFF && q + 8 <= x + 4 + lg) { compresse = U64(rep, q); q += 8; }
                        if (local == 0xFFFFFFFF && q + 8 <= x + 4 + lg) { local = U64(rep, q); }
                    }
                    x += 4 + lg;
                }
                _entrees.Add(new ZipArchiveEntry(this, nom, methode, drapeaux, crc, compresse, clair, local, DateDos(dateDos, heureDos)));
                p += 46 + lnom + lextra + lcomm;
            }
        }

        // 4.4.6 : date et heure MS-DOS ; une valeur impossible donne 1980-01-01.
        private static DateTimeOffset DateDos(int date, int heure)
        {
            try
            {
                DateTime d = new DateTime(1980 + (date >> 9), (date >> 5) & 15, date & 31,
                    heure >> 11, (heure >> 5) & 63, (heure & 31) * 2);
                return new DateTimeOffset(d);
            }
            catch (ArgumentOutOfRangeException)
            {
                return new DateTimeOffset(new DateTime(1980, 1, 1));
            }
        }
    }

    public class ZipArchiveEntry
    {
        private readonly ZipArchive _archive;
        private readonly string _nom;
        private readonly int _methode, _drapeaux;
        private readonly uint _crc;
        private readonly long _compresse, _clair, _local;
        private readonly DateTimeOffset _date;

        internal ZipArchiveEntry(ZipArchive archive, string nom, int methode, int drapeaux, uint crc,
            long compresse, long clair, long local, DateTimeOffset date)
        {
            _archive = archive;
            _nom = nom;
            _methode = methode;
            _drapeaux = drapeaux;
            _crc = crc;
            _compresse = compresse;
            _clair = clair;
            _local = local;
            _date = date;
        }

        public ZipArchive Archive { get { return _archive; } }
        public string FullName { get { return _nom; } }

        // Learn : le nom de fichier, après le dernier séparateur.
        public string Name
        {
            get
            {
                int i = Math.Max(_nom.LastIndexOf('/'), _nom.LastIndexOf('\\'));
                return _nom.Substring(i + 1);
            }
        }

        public long Length { get { return _clair; } }
        public long CompressedLength { get { return _compresse; } }
        public DateTimeOffset LastWriteTime { get { return _date; } set { throw new NotSupportedException("Cannot modify read-only archive."); } }
        public uint Crc32 { get { return _crc; } }

        public Stream Open()
        {
            Stream f = _archive.Flux;
            if ((_drapeaux & 1) != 0) throw new InvalidDataException("The archive entry was compressed using an unsupported compression method.");
            // 4.3.7 : en-tête local, 30 octets fixes puis nom et extra.
            byte[] l = new byte[30];
            try { _archive.LireExact(_local, l); }
            catch (EndOfStreamException e) { throw new InvalidDataException("A local file header is corrupt.", e); }
            if ((l[0] | (l[1] << 8) | (l[2] << 16) | (l[3] << 24)) != 0x04034b50)
                throw new InvalidDataException("A local file header is corrupt.");
            long donnees = _local + 30 + (l[26] | (l[27] << 8)) + (l[28] | (l[29] << 8));
            if (donnees + _compresse > f.Length) throw new InvalidDataException("A local file header is corrupt.");
            SousFlux brut = new SousFlux(f, donnees, _compresse);
            if (_methode == 0) return brut;
            if (_methode == 8) return new DeflateStream(brut, CompressionMode.Decompress);
            throw new InvalidDataException("The archive entry was compressed using an unsupported compression method.");
        }

        public void Delete() { throw new NotSupportedException("Cannot modify read-only archive."); }

        public override string ToString() { return _nom; }
    }
}
