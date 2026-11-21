// Façade de compilation « System.IO.Compression » pour les témoins de la BCL
// Winlux (#2819). Licence GPL 3. Jamais chargée ni livrée : un témoin compilé
// contre elle référence ZipArchive dans l'assembly « System.IO.Compression »,
// comme un programme .NET Framework, et le moteur doit le trouver dans notre
// mscorlib.
[assembly: System.Reflection.AssemblyVersion("4.0.0.0")]

namespace System.IO.Compression
{
    public enum CompressionMode { Decompress = 0, Compress = 1 }

    public enum ZipArchiveMode { Read = 0, Create = 1, Update = 2 }

    public class DeflateStream : Stream
    {
        public DeflateStream(Stream stream, CompressionMode mode) { throw new InvalidOperationException("façade"); }
        public override bool CanRead { get { throw new InvalidOperationException("façade"); } }
        public override bool CanSeek { get { throw new InvalidOperationException("façade"); } }
        public override bool CanWrite { get { throw new InvalidOperationException("façade"); } }
        public override long Length { get { throw new InvalidOperationException("façade"); } }
        public override long Position { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public override void Flush() { throw new InvalidOperationException("façade"); }
        public override int Read(byte[] buffer, int offset, int count) { throw new InvalidOperationException("façade"); }
        public override long Seek(long offset, SeekOrigin origin) { throw new InvalidOperationException("façade"); }
        public override void SetLength(long value) { throw new InvalidOperationException("façade"); }
        public override void Write(byte[] buffer, int offset, int count) { throw new InvalidOperationException("façade"); }
    }

    public class ZipArchive : IDisposable
    {
        public ZipArchive(Stream stream) { throw new InvalidOperationException("façade"); }
        public ZipArchive(Stream stream, ZipArchiveMode mode) { throw new InvalidOperationException("façade"); }
        public System.Collections.ObjectModel.ReadOnlyCollection<ZipArchiveEntry> Entries { get { throw new InvalidOperationException("façade"); } }
        public ZipArchiveEntry GetEntry(string entryName) { throw new InvalidOperationException("façade"); }
        public void Dispose() { throw new InvalidOperationException("façade"); }
    }

    public class ZipArchiveEntry
    {
        public string FullName { get { throw new InvalidOperationException("façade"); } }
        public string Name { get { throw new InvalidOperationException("façade"); } }
        public long Length { get { throw new InvalidOperationException("façade"); } }
        public DateTimeOffset LastWriteTime { get { throw new InvalidOperationException("façade"); } }
        public Stream Open() { throw new InvalidOperationException("façade"); }
    }
}
