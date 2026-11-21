// Façade de compilation « System » pour les témoins de la BCL Winlux (#1947, #2814).
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

namespace System
{
    public class Uri
    {
        public Uri(string uriString) { throw new InvalidOperationException("façade"); }
        public string Scheme { get { throw new InvalidOperationException("façade"); } }
        public string Host { get { throw new InvalidOperationException("façade"); } }
        public int Port { get { throw new InvalidOperationException("façade"); } }
        public string PathAndQuery { get { throw new InvalidOperationException("façade"); } }
        public string AbsoluteUri { get { throw new InvalidOperationException("façade"); } }
        public Uri(string uriString, UriKind uriKind) { throw new InvalidOperationException("façade"); }
        public Uri(Uri baseUri, string relativeUri) { throw new InvalidOperationException("façade"); }
        public Uri(Uri baseUri, Uri relativeUri) { throw new InvalidOperationException("façade"); }
        public static bool TryCreate(string uriString, UriKind uriKind, out Uri result) { throw new InvalidOperationException("façade"); }
        public static bool TryCreate(Uri baseUri, string relativeUri, out Uri result) { throw new InvalidOperationException("façade"); }
        public static bool TryCreate(Uri baseUri, Uri relativeUri, out Uri result) { throw new InvalidOperationException("façade"); }
        public bool IsAbsoluteUri { get { throw new InvalidOperationException("façade"); } }
        public string OriginalString { get { throw new InvalidOperationException("façade"); } }
        public static bool operator ==(Uri a, Uri b) { throw new InvalidOperationException("façade"); }
        public static bool operator !=(Uri a, Uri b) { throw new InvalidOperationException("façade"); }
        public override bool Equals(object o) { throw new InvalidOperationException("façade"); }
        public override int GetHashCode() { throw new InvalidOperationException("façade"); }
    }

    public enum UriKind { RelativeOrAbsolute = 0, Absolute = 1, Relative = 2 }

    public class UriFormatException : FormatException { }
}

namespace System.Net
{
    public interface ICredentials { }

    public class CredentialCache
    {
        public static ICredentials DefaultCredentials { get { throw new InvalidOperationException("façade"); } }
    }

    public interface IWebProxy
    {
        ICredentials Credentials { get; set; }
    }

    public class WebProxy : IWebProxy
    {
        public WebProxy() { throw new InvalidOperationException("façade"); }
        public WebProxy(string Address) { throw new InvalidOperationException("façade"); }
        public ICredentials Credentials { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
    }

    public enum WebExceptionStatus { Success = 0, NameResolutionFailure = 1, ConnectFailure = 2, ProtocolError = 7, Timeout = 14, UnknownError = 16 }

    public enum HttpStatusCode { OK = 200, PartialContent = 206, NotFound = 404 }

    public class WebException : InvalidOperationException
    {
        public WebExceptionStatus Status { get { throw new InvalidOperationException("façade"); } }
        public WebResponse Response { get { throw new InvalidOperationException("façade"); } }
    }

    public abstract class WebResponse : IDisposable
    {
        public virtual long ContentLength { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public virtual System.IO.Stream GetResponseStream() { throw new InvalidOperationException("façade"); }
        public virtual void Close() { throw new InvalidOperationException("façade"); }
        public void Dispose() { throw new InvalidOperationException("façade"); }
    }

    public abstract class WebRequest
    {
        public static WebRequest Create(string requestUriString) { throw new InvalidOperationException("façade"); }
        public static WebRequest Create(Uri requestUri) { throw new InvalidOperationException("façade"); }
        public virtual int Timeout { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public virtual IWebProxy Proxy { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
        public virtual WebResponse GetResponse() { throw new InvalidOperationException("façade"); }
    }

    public class HttpWebRequest : WebRequest
    {
        public void AddRange(int range) { throw new InvalidOperationException("façade"); }
        public void AddRange(long from, long to) { throw new InvalidOperationException("façade"); }
        public string UserAgent { get { throw new InvalidOperationException("façade"); } set { throw new InvalidOperationException("façade"); } }
    }

    public class HttpWebResponse : WebResponse
    {
        public HttpStatusCode StatusCode { get { throw new InvalidOperationException("façade"); } }
        public string GetResponseHeader(string headerName) { throw new InvalidOperationException("façade"); }
    }
}
