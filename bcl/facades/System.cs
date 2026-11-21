// Façade de compilation « System » pour les témoins de la BCL Winlux (#1947).
// Licence GPL 3. Elle n'est jamais chargée ni livrée : un témoin compilé
// contre elle référence ses types dans l'assembly « System », comme un
// programme .NET Framework, et le moteur doit alors les trouver dans notre
// mscorlib (System.Diagnostics.Stopwatch est dans System.dll).

[assembly: System.Reflection.AssemblyVersion("4.0.0.0")]

namespace System.Diagnostics
{
    public class Stopwatch
    {
        public static Stopwatch StartNew() { throw new InvalidOperationException("façade"); }
        public void Stop() { throw new InvalidOperationException("façade"); }
        public long ElapsedTicks { get { throw new InvalidOperationException("façade"); } }
        public bool IsRunning { get { throw new InvalidOperationException("façade"); } }
    }

    public sealed class FileVersionInfo
    {
        public static FileVersionInfo GetVersionInfo(string fileName) { throw new InvalidOperationException("façade"); }
        public string FileName { get { throw new InvalidOperationException("façade"); } }
        public string FileVersion { get { throw new InvalidOperationException("façade"); } }
        public string ProductVersion { get { throw new InvalidOperationException("façade"); } }
        public string CompanyName { get { throw new InvalidOperationException("façade"); } }
        public string ProductName { get { throw new InvalidOperationException("façade"); } }
        public string FileDescription { get { throw new InvalidOperationException("façade"); } }
        public string LegalCopyright { get { throw new InvalidOperationException("façade"); } }
        public string OriginalFilename { get { throw new InvalidOperationException("façade"); } }
        public int FileMajorPart { get { throw new InvalidOperationException("façade"); } }
        public int FileMinorPart { get { throw new InvalidOperationException("façade"); } }
        public int FileBuildPart { get { throw new InvalidOperationException("façade"); } }
        public int FilePrivatePart { get { throw new InvalidOperationException("façade"); } }
        public int ProductMajorPart { get { throw new InvalidOperationException("façade"); } }
        public bool IsDebug { get { throw new InvalidOperationException("façade"); } }
    }
}
