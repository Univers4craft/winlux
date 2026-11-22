// System.Uri et System.Net (WebRequest, HttpWebRequest, HttpWebResponse,
// WebProxy, CredentialCache, WebException) de la mscorlib Winlux (#2814,
// suivi #2752), sur le WinHTTP de Winlux (P/Invoke). Licence GPL 3.
//
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « Uri Class », « WebRequest.Create », « HttpWebRequest Class »,
// « HttpWebRequest.AddRange », « HttpWebResponse Class »,
// « HttpWebResponse.GetResponseHeader », « WebResponse.GetResponseStream »,
// « WebException », « WebExceptionStatus », « HttpStatusCode », « WebProxy
// Class », « CredentialCache.DefaultCredentials ») et celle de WinHTTP
// (WinHttpOpen, WinHttpConnect, WinHttpOpenRequest, WinHttpSendRequest,
// WinHttpReceiveResponse, WinHttpQueryHeaders, WinHttpReadData) ; aucun code
// de Microsoft, de Mono ou de Wine.
//
// Uri ne sait ici que les URI absolues « schéma://[userinfo@]hôte[:port]/… »
// (http, https, ftp, file) : c'est ce que demandent les téléchargeurs.
using System;
using System.IO;
using System.Runtime.InteropServices;

namespace System
{
    // Learn « NotImplementedException » : ce que lèvent les membres de base
    // de WebRequest et WebResponse.
    public class NotImplementedException : SystemException
    {
        public NotImplementedException() : base("The method or operation is not implemented.") { }
        public NotImplementedException(string message) : base(message) { }
        public NotImplementedException(string message, Exception inner) : base(message, inner) { }
    }

    public enum UriKind { RelativeOrAbsolute = 0, Absolute = 1, Relative = 2 }

    public class UriFormatException : FormatException
    {
        public UriFormatException() { }
        public UriFormatException(string textString) : base(textString) { }
    }

    public class Uri
    {
        private string _original;
        private string _schema, _hote, _chemin, _requete, _fragment, _utilisateur;
        private int _port;
        private bool _portExplicite;
        // Référence relative (Learn « Uri.IsAbsoluteUri ») : seul le texte
        // d'origine est gardé (#2844).
        private bool _relative;

        public static readonly string UriSchemeHttp = "http";
        public static readonly string UriSchemeHttps = "https";
        public static readonly string UriSchemeFtp = "ftp";
        public static readonly string UriSchemeFile = "file";

        private Uri() { }

        public Uri(string uriString) : this(uriString, UriKind.Absolute) { }

        public Uri(string uriString, UriKind uriKind)
        {
            if (uriString == null) throw new ArgumentNullException("uriString");
            string erreur = Analyser(uriString, uriKind);
            if (erreur != null) throw new UriFormatException(erreur);
        }

        // Learn « Uri(Uri, String) » : la référence résolue sur la base
        // (RFC 3986 §5.2.2).
        public Uri(Uri baseUri, string relativeUri)
        {
            if ((object)baseUri == null) throw new ArgumentNullException("baseUri");
            if (!baseUri.IsAbsoluteUri) throw new ArgumentOutOfRangeException("baseUri");
            string erreur = Analyser(Resoudre(baseUri, relativeUri ?? ""), UriKind.Absolute);
            if (erreur != null) throw new UriFormatException(erreur);
        }

        public Uri(Uri baseUri, Uri relativeUri)
            : this(baseUri, (object)relativeUri == null ? null : relativeUri.OriginalString)
        {
            if ((object)relativeUri == null) throw new ArgumentNullException("relativeUri");
        }

        // Learn « Uri.TryCreate » : false au lieu d'une UriFormatException.
        public static bool TryCreate(string uriString, UriKind uriKind, out Uri result)
        {
            result = null;
            if (uriString == null) return false;
            Uri u = new Uri();
            if (u.Analyser(uriString, uriKind) != null) return false;
            result = u;
            return true;
        }

        public static bool TryCreate(Uri baseUri, string relativeUri, out Uri result)
        {
            result = null;
            if ((object)baseUri == null || !baseUri.IsAbsoluteUri || relativeUri == null) return false;
            return TryCreate(Resoudre(baseUri, relativeUri), UriKind.Absolute, out result);
        }

        public static bool TryCreate(Uri baseUri, Uri relativeUri, out Uri result)
        {
            result = null;
            if ((object)relativeUri == null) return false;
            if (relativeUri.IsAbsoluteUri) { result = relativeUri; return true; }
            return TryCreate(baseUri, relativeUri.OriginalString, out result);
        }

        // Rend null, ou le message de l'UriFormatException.
        private string Analyser(string uriString, UriKind uriKind)
        {
            string s = uriString.Trim();
            int sep = s.IndexOf("://", StringComparison.Ordinal);
            bool absolue = sep > 0 && SchemaValide(s.Substring(0, sep));
            if (!absolue)
            {
                if (uriKind == UriKind.Absolute || s.IndexOf("://", StringComparison.Ordinal) == 0)
                    return "Invalid URI: The format of the URI could not be determined.";
                _original = uriString;
                _relative = true;
                return null;
            }
            if (uriKind == UriKind.Relative) return "Invalid URI: A relative URI cannot be created because the 'uriString' parameter represents an absolute URI.";
            _original = uriString;
            _schema = s.Substring(0, sep).ToLowerInvariant();
            string reste = s.Substring(sep + 3);
            int fin = reste.Length;
            foreach (char c in new char[] { '/', '?', '#' })
            {
                int i = reste.IndexOf(c);
                if (i >= 0 && i < fin) fin = i;
            }
            string autorite = reste.Substring(0, fin);
            string suite = reste.Substring(fin);
            int arobase = autorite.LastIndexOf('@');
            _utilisateur = arobase >= 0 ? autorite.Substring(0, arobase) : "";
            if (arobase >= 0) autorite = autorite.Substring(arobase + 1);
            int deuxPoints = autorite.StartsWith("[") ? autorite.IndexOf("]:", StringComparison.Ordinal) + 1 : autorite.LastIndexOf(':');
            _port = PortParDefaut(_schema);
            if (deuxPoints > 0)
            {
                int p;
                if (!int.TryParse(autorite.Substring(deuxPoints + 1), out p) || p < 0 || p > 65535)
                    return "Invalid URI: Invalid port specified.";
                _port = p;
                _portExplicite = true;
                autorite = autorite.Substring(0, deuxPoints);
            }
            if (autorite.Length == 0 && _schema != "file")
                return "Invalid URI: The hostname could not be parsed.";
            _hote = autorite.ToLowerInvariant();
            int diese = suite.IndexOf('#');
            _fragment = diese >= 0 ? suite.Substring(diese) : "";
            if (diese >= 0) suite = suite.Substring(0, diese);
            int question = suite.IndexOf('?');
            _requete = question >= 0 ? suite.Substring(question) : "";
            _chemin = question >= 0 ? suite.Substring(0, question) : suite;
            if (_chemin.Length == 0) _chemin = "/";
            _chemin = _chemin.Replace(" ", "%20");
            _requete = _requete.Replace(" ", "%20");
            return null;
        }

        private static bool SchemaValide(string schema)
        {
            if (schema.Length == 0) return false;
            for (int i = 0; i < schema.Length; i++)
            {
                char c = schema[i];
                if (!(char.IsLetterOrDigit(c) || c == '+' || c == '-' || c == '.') || (i == 0 && !char.IsLetter(c)))
                    return false;
            }
            return true;
        }

        // RFC 3986 §5.2.2 à 5.2.4 : résolution d'une référence sur une base
        // absolue, en texte.
        private static string Resoudre(Uri b, string r)
        {
            r = r.Trim();
            int sep = r.IndexOf("://", StringComparison.Ordinal);
            if (sep > 0 && SchemaValide(r.Substring(0, sep))) return r;
            string debut = b._schema + "://";
            if (r.StartsWith("//")) return b._schema + ":" + r;
            string autorite = (b._utilisateur.Length > 0 ? b._utilisateur + "@" : "") + b.Authority;
            string fragment = "";
            int diese = r.IndexOf('#');
            if (diese >= 0) { fragment = r.Substring(diese); r = r.Substring(0, diese); }
            string requete = null;
            int question = r.IndexOf('?');
            if (question >= 0) { requete = r.Substring(question); r = r.Substring(0, question); }
            string chemin;
            if (r.Length == 0)
            {
                chemin = b._chemin;
                if (requete == null) requete = b._requete;
            }
            else if (r[0] == '/') chemin = SansPoints(r);
            else
            {
                int barre = b._chemin.LastIndexOf('/');
                chemin = SansPoints((barre >= 0 ? b._chemin.Substring(0, barre + 1) : "/") + r);
            }
            return debut + autorite + chemin + (requete ?? "") + fragment;
        }

        // RFC 3986 §5.2.4 « remove_dot_segments ».
        private static string SansPoints(string entree)
        {
            System.Text.StringBuilder sortie = new System.Text.StringBuilder();
            while (entree.Length > 0)
            {
                if (entree.StartsWith("../")) entree = entree.Substring(3);
                else if (entree.StartsWith("./")) entree = entree.Substring(2);
                else if (entree.StartsWith("/./")) entree = entree.Substring(2);
                else if (entree == "/.") entree = "/";
                else if (entree.StartsWith("/../") || entree == "/..")
                {
                    entree = entree == "/.." ? "/" : entree.Substring(3);
                    string s = sortie.ToString();
                    int i = s.LastIndexOf('/');
                    sortie.Length = i >= 0 ? i : 0;
                }
                else if (entree == "." || entree == "..") entree = "";
                else
                {
                    int i = entree.IndexOf('/', entree[0] == '/' ? 1 : 0);
                    string segment = i < 0 ? entree : entree.Substring(0, i);
                    sortie.Append(segment);
                    entree = i < 0 ? "" : entree.Substring(i);
                }
            }
            return sortie.ToString();
        }

        private static int PortParDefaut(string schema)
        {
            switch (schema)
            {
                case "http": return 80;
                case "https": return 443;
                case "ftp": return 21;
                default: return -1;
            }
        }

        // Learn : ces propriétés lèvent InvalidOperationException sur une
        // référence relative.
        private void Absolue()
        {
            if (_relative) throw new InvalidOperationException("This operation is not supported for a relative URI.");
        }

        public string Scheme { get { Absolue(); return _schema; } }
        public string Host { get { Absolue(); return _hote; } }
        public string DnsSafeHost { get { Absolue(); return _hote.Trim('[', ']'); } }
        public int Port { get { Absolue(); return _port; } }
        public bool IsDefaultPort { get { Absolue(); return _port == PortParDefaut(_schema); } }
        public string AbsolutePath { get { Absolue(); return _chemin; } }
        public string Query { get { Absolue(); return _requete; } }
        public string Fragment { get { Absolue(); return _fragment; } }
        public string PathAndQuery { get { Absolue(); return _chemin + _requete; } }
        public string UserInfo { get { Absolue(); return _utilisateur; } }
        public string OriginalString { get { return _original; } }
        public bool IsAbsoluteUri { get { return !_relative; } }
        public bool IsFile { get { Absolue(); return _schema == "file"; } }
        public bool IsLoopback { get { Absolue(); return _hote == "localhost" || _hote == "127.0.0.1" || _hote == "[::1]"; } }
        public string LocalPath { get { Absolue(); return IsFile ? Uri.UnescapeDataString(_chemin) : _chemin; } }

        public string Authority
        {
            get { Absolue(); return _portExplicite && !IsDefaultPort ? _hote + ":" + _port : _hote; }
        }

        public string AbsoluteUri
        {
            get { Absolue(); return _schema + "://" + (_utilisateur.Length > 0 ? _utilisateur + "@" : "") + Authority + _chemin + _requete + _fragment; }
        }

        public string[] Segments
        {
            get
            {
                Absolue();
                System.Collections.Generic.List<string> l = new System.Collections.Generic.List<string>();
                int debut = 0;
                for (int i = 0; i < _chemin.Length; i++)
                {
                    if (_chemin[i] == '/') { l.Add(_chemin.Substring(debut, i - debut + 1)); debut = i + 1; }
                }
                if (debut < _chemin.Length) l.Add(_chemin.Substring(debut));
                return l.ToArray();
            }
        }

        public static string UnescapeDataString(string stringToUnescape)
        {
            if (stringToUnescape == null) throw new ArgumentNullException("stringToUnescape");
            if (stringToUnescape.IndexOf('%') < 0) return stringToUnescape;
            System.Collections.Generic.List<byte> octets = new System.Collections.Generic.List<byte>();
            System.Text.StringBuilder sb = new System.Text.StringBuilder();
            int i = 0;
            while (i < stringToUnescape.Length)
            {
                int v;
                if (stringToUnescape[i] == '%' && i + 2 < stringToUnescape.Length
                    && int.TryParse(stringToUnescape.Substring(i + 1, 2), System.Globalization.NumberStyles.HexNumber, null, out v))
                {
                    octets.Add((byte)v);
                    i += 3;
                    continue;
                }
                if (octets.Count > 0) { sb.Append(System.Text.Encoding.UTF8.GetString(octets.ToArray())); octets.Clear(); }
                sb.Append(stringToUnescape[i]);
                i++;
            }
            if (octets.Count > 0) sb.Append(System.Text.Encoding.UTF8.GetString(octets.ToArray()));
            return sb.ToString();
        }

        public override string ToString() { return _relative ? _original : AbsoluteUri; }

        public override bool Equals(object comparand)
        {
            Uri u = comparand as Uri;
            if (u == null && comparand is string) { if (!TryCreate((string)comparand, UriKind.RelativeOrAbsolute, out u)) return false; }
            if (u == null || u._relative != _relative) return false;
            if (_relative) return u._original == _original;
            return u._schema == _schema && u._hote == _hote && u._port == _port && u._chemin == _chemin && u._requete == _requete;
        }

        public override int GetHashCode() { return ToString().GetHashCode(); }

        public static bool operator ==(Uri a, Uri b) { return (object)a == null ? (object)b == null : a.Equals(b); }
        public static bool operator !=(Uri a, Uri b) { return !(a == b); }
    }
}

namespace System.Net
{
    public interface ICredentials
    {
        NetworkCredential GetCredential(Uri uri, string authType);
    }

    public class NetworkCredential : ICredentials
    {
        public NetworkCredential() { UserName = ""; Password = ""; Domain = ""; }
        public NetworkCredential(string userName, string password) : this(userName, password, "") { }
        public NetworkCredential(string userName, string password, string domain)
        {
            UserName = userName ?? ""; Password = password ?? ""; Domain = domain ?? "";
        }
        public string UserName { get; set; }
        public string Password { get; set; }
        public string Domain { get; set; }
        public NetworkCredential GetCredential(Uri uri, string authType) { return this; }
    }

    public class CredentialCache : ICredentials
    {
        private static readonly NetworkCredential s_systeme = new NetworkCredential();

        // Learn : les identifiants du contexte de sécurité courant ; vides
        // ici, la session locale n'en transmet pas (pas d'annuaire).
        public static ICredentials DefaultCredentials { get { return s_systeme; } }
        public static NetworkCredential DefaultNetworkCredentials { get { return s_systeme; } }

        public NetworkCredential GetCredential(Uri uri, string authType) { return null; }
    }

    public interface IWebProxy
    {
        ICredentials Credentials { get; set; }
        Uri GetProxy(Uri destination);
        bool IsBypassed(Uri host);
    }

    public class WebProxy : IWebProxy
    {
        private Uri _adresse;

        public WebProxy() { }
        public WebProxy(Uri Address) { _adresse = Address; }
        public WebProxy(string Address) { if (Address != null) _adresse = Adresse(Address); }
        public WebProxy(string Host, int Port) { _adresse = new Uri("http://" + Host + ":" + Port); }
        public WebProxy(string Address, bool BypassOnLocal) : this(Address) { BypassProxyOnLocal = BypassOnLocal; }

        private static Uri Adresse(string a) { return new Uri(a.Contains("://") ? a : "http://" + a); }

        public Uri Address { get { return _adresse; } set { _adresse = value; } }
        public bool BypassProxyOnLocal { get; set; }
        public ICredentials Credentials { get; set; }

        public bool UseDefaultCredentials
        {
            get { return Credentials == CredentialCache.DefaultCredentials; }
            set { Credentials = value ? CredentialCache.DefaultCredentials : null; }
        }

        // Learn : sans adresse, ou pour une destination contournée, la
        // destination elle-même (pas de mandataire).
        public Uri GetProxy(Uri destination)
        {
            if (destination == null) throw new ArgumentNullException("destination");
            return IsBypassed(destination) ? destination : _adresse;
        }

        public bool IsBypassed(Uri host)
        {
            if (host == null) throw new ArgumentNullException("host");
            return _adresse == null || (BypassProxyOnLocal && host.IsLoopback);
        }
    }

    public enum WebExceptionStatus
    {
        Success = 0, NameResolutionFailure = 1, ConnectFailure = 2, ReceiveFailure = 3, SendFailure = 4,
        PipelineFailure = 5, RequestCanceled = 6, ProtocolError = 7, ConnectionClosed = 8, TrustFailure = 9,
        SecureChannelFailure = 10, ServerProtocolViolation = 11, KeepAliveFailure = 12, Pending = 13, Timeout = 14,
        ProxyNameResolutionFailure = 15, UnknownError = 16, MessageLengthLimitExceeded = 17, CacheEntryNotFound = 18,
        RequestProhibitedByCachePolicy = 19, RequestProhibitedByProxy = 20
    }

    public enum HttpStatusCode
    {
        Continue = 100, SwitchingProtocols = 101, OK = 200, Created = 201, Accepted = 202,
        NonAuthoritativeInformation = 203, NoContent = 204, ResetContent = 205, PartialContent = 206,
        MultipleChoices = 300, Ambiguous = 300, MovedPermanently = 301, Moved = 301, Found = 302, Redirect = 302,
        SeeOther = 303, RedirectMethod = 303, NotModified = 304, UseProxy = 305, Unused = 306,
        TemporaryRedirect = 307, RedirectKeepVerb = 307, BadRequest = 400, Unauthorized = 401,
        PaymentRequired = 402, Forbidden = 403, NotFound = 404, MethodNotAllowed = 405, NotAcceptable = 406,
        ProxyAuthenticationRequired = 407, RequestTimeout = 408, Conflict = 409, Gone = 410, LengthRequired = 411,
        PreconditionFailed = 412, RequestEntityTooLarge = 413, RequestUriTooLong = 414,
        UnsupportedMediaType = 415, RequestedRangeNotSatisfiable = 416, ExpectationFailed = 417,
        UpgradeRequired = 426, InternalServerError = 500, NotImplemented = 501, BadGateway = 502,
        ServiceUnavailable = 503, GatewayTimeout = 504, HttpVersionNotSupported = 505
    }

    public class WebException : InvalidOperationException
    {
        private readonly WebExceptionStatus _statut;
        private readonly WebResponse _reponse;

        public WebException() : this("An exception occurred during a WebClient request.") { }
        public WebException(string message) : this(message, WebExceptionStatus.UnknownError) { }
        public WebException(string message, Exception innerException) : base(message, innerException) { _statut = WebExceptionStatus.UnknownError; }
        public WebException(string message, WebExceptionStatus status) : this(message, null, status, null) { }

        public WebException(string message, Exception innerException, WebExceptionStatus status, WebResponse response)
            : base(message, innerException)
        {
            _statut = status;
            _reponse = response;
        }

        public WebExceptionStatus Status { get { return _statut; } }
        public WebResponse Response { get { return _reponse; } }
    }

    public abstract class WebResponse : IDisposable
    {
        protected WebResponse() { }
        public virtual long ContentLength { get { throw new NotImplementedException(); } set { throw new NotImplementedException(); } }
        public virtual string ContentType { get { throw new NotImplementedException(); } set { throw new NotImplementedException(); } }
        public virtual Uri ResponseUri { get { throw new NotImplementedException(); } }
        public virtual Stream GetResponseStream() { throw new NotImplementedException(); }
        public virtual void Close() { }
        public void Dispose() { Close(); }
    }

    public abstract class WebRequest
    {
        private static IWebProxy s_mandataire = new WebProxy();

        protected WebRequest() { }

        // Learn : http et https donnent une HttpWebRequest ; un autre préfixe
        // lève NotSupportedException.
        public static WebRequest Create(string requestUriString)
        {
            if (requestUriString == null) throw new ArgumentNullException("requestUriString");
            return Create(new Uri(requestUriString));
        }

        public static WebRequest Create(Uri requestUri)
        {
            if (requestUri == null) throw new ArgumentNullException("requestUri");
            if (requestUri.Scheme == "http" || requestUri.Scheme == "https") return new HttpWebRequest(requestUri);
            throw new NotSupportedException("The URI prefix is not recognized.");
        }

        public static HttpWebRequest CreateHttp(string requestUriString) { return (HttpWebRequest)Create(requestUriString); }
        public static HttpWebRequest CreateHttp(Uri requestUri) { return (HttpWebRequest)Create(requestUri); }

        // Mandataire par défaut : celui du système, que notre WinHTTP applique
        // de lui-même (WINHTTP_ACCESS_TYPE_DEFAULT_PROXY).
        public static IWebProxy DefaultWebProxy { get { return s_mandataire; } set { s_mandataire = value; } }

        public virtual string Method { get { throw new NotImplementedException(); } set { throw new NotImplementedException(); } }
        public virtual Uri RequestUri { get { throw new NotImplementedException(); } }
        public virtual int Timeout { get { throw new NotImplementedException(); } set { throw new NotImplementedException(); } }
        public virtual IWebProxy Proxy { get { throw new NotImplementedException(); } set { throw new NotImplementedException(); } }
        public virtual ICredentials Credentials { get { throw new NotImplementedException(); } set { throw new NotImplementedException(); } }
        public virtual bool UseDefaultCredentials { get { throw new NotImplementedException(); } set { throw new NotImplementedException(); } }
        public virtual string ContentType { get { throw new NotImplementedException(); } set { throw new NotImplementedException(); } }
        public virtual WebResponse GetResponse() { throw new NotImplementedException(); }
        public virtual void Abort() { throw new NotImplementedException(); }
    }

    internal static class WinHttp
    {
        [DllImport("winhttp.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr WinHttpOpen(string agent, int accessType, string proxyName, string proxyBypass, int flags);
        [DllImport("winhttp.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr WinHttpConnect(IntPtr session, string server, int port, int reserved);
        [DllImport("winhttp.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr WinHttpOpenRequest(IntPtr connect, string verb, string objectName, string version,
            string referrer, IntPtr acceptTypes, int flags);
        [DllImport("winhttp.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool WinHttpAddRequestHeaders(IntPtr request, string headers, int length, int modifiers);
        [DllImport("winhttp.dll", SetLastError = true)]
        public static extern bool WinHttpSetTimeouts(IntPtr handle, int resolve, int connect, int send, int receive);
        [DllImport("winhttp.dll", SetLastError = true)]
        public static extern bool WinHttpSendRequest(IntPtr request, IntPtr headers, int headersLength, IntPtr optional,
            int optionalLength, int totalLength, IntPtr context);
        [DllImport("winhttp.dll", SetLastError = true)]
        public static extern bool WinHttpReceiveResponse(IntPtr request, IntPtr reserved);
        [DllImport("winhttp.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern bool WinHttpQueryHeaders(IntPtr request, int infoLevel, string name, IntPtr buffer,
            IntPtr bufferLength, IntPtr index);
        [DllImport("winhttp.dll", SetLastError = true)]
        public static extern bool WinHttpReadData(IntPtr request, IntPtr buffer, int toRead, IntPtr read);
        [DllImport("winhttp.dll", SetLastError = true)]
        public static extern bool WinHttpSetCredentials(IntPtr request, int targets, int scheme, IntPtr user, IntPtr password, IntPtr param);
        [DllImport("winhttp.dll")]
        public static extern bool WinHttpCloseHandle(IntPtr handle);

        public const int QUERY_CONTENT_TYPE = 1, QUERY_CONTENT_LENGTH = 5, QUERY_STATUS_CODE = 19,
            QUERY_STATUS_TEXT = 20, QUERY_CUSTOM = 65535, QUERY_FLAG_NUMBER = 0x20000000;

        // En-tête texte, ou null s'il manque.
        public static string Entete(IntPtr requete, int niveau, string nom)
        {
            IntPtr taille = Marshal.AllocHGlobal(4);
            try
            {
                Marshal.WriteInt32(taille, 0);
                WinHttpQueryHeaders(requete, niveau, nom, IntPtr.Zero, taille, IntPtr.Zero);
                int n = Marshal.ReadInt32(taille);
                if (n <= 0) return null;
                IntPtr tampon = Marshal.AllocHGlobal(n + 2);
                try
                {
                    if (!WinHttpQueryHeaders(requete, niveau, nom, tampon, taille, IntPtr.Zero)) return null;
                    return Marshal.PtrToStringUni(tampon, Marshal.ReadInt32(taille) / 2);
                }
                finally { Marshal.FreeHGlobal(tampon); }
            }
            finally { Marshal.FreeHGlobal(taille); }
        }

        // Erreur WinHTTP → WebException (Learn « WebExceptionStatus »).
        public static WebException Echec(int erreur)
        {
            switch (erreur)
            {
                case 12007: return new WebException("The remote name could not be resolved", WebExceptionStatus.NameResolutionFailure);
                case 12002: return new WebException("The operation has timed out", WebExceptionStatus.Timeout);
                case 12029:
                case 12030: return new WebException("Unable to connect to the remote server", WebExceptionStatus.ConnectFailure);
                case 12017: return new WebException("The request was aborted: The request was canceled.", WebExceptionStatus.RequestCanceled);
                case 12175:
                case 12045:
                case 12169: return new WebException("The underlying connection was closed: Could not establish trust relationship for the SSL/TLS secure channel.", WebExceptionStatus.TrustFailure);
                default: return new WebException("An error occurred while sending the request (WinHTTP " + erreur + ").", WebExceptionStatus.UnknownError);
            }
        }
    }

    public class HttpWebRequest : WebRequest
    {
        private readonly Uri _uri;
        private string _methode = "GET";
        private int _delai = 100000, _delaiLecture = 300000;
        private IWebProxy _mandataire;
        private bool _mandatairePose;
        private ICredentials _identifiants;
        private string _plage;
        private readonly System.Collections.Generic.List<string> _entetes = new System.Collections.Generic.List<string>();
        private IntPtr _session, _connexion, _requete;

        internal HttpWebRequest(Uri uri)
        {
            _uri = uri;
            UserAgent = null;
            AllowAutoRedirect = true;
            KeepAlive = true;
            MaximumAutomaticRedirections = 50;
        }

        public override Uri RequestUri { get { return _uri; } }
        public Uri Address { get { return _uri; } }

        public override string Method
        {
            get { return _methode; }
            set
            {
                if (string.IsNullOrEmpty(value)) throw new ArgumentException("Method must be set.", "value");
                _methode = value.ToUpperInvariant();
            }
        }

        // Learn : millisecondes, Timeout.Infinite (-1) ou positif.
        public override int Timeout
        {
            get { return _delai; }
            set
            {
                if (value < 0 && value != -1) throw new ArgumentOutOfRangeException("value");
                _delai = value;
            }
        }

        public int ReadWriteTimeout
        {
            get { return _delaiLecture; }
            set
            {
                if (value <= 0 && value != -1) throw new ArgumentOutOfRangeException("value");
                _delaiLecture = value;
            }
        }

        public override IWebProxy Proxy
        {
            get { return _mandatairePose ? _mandataire : WebRequest.DefaultWebProxy; }
            set { _mandataire = value; _mandatairePose = true; }
        }

        public override ICredentials Credentials { get { return _identifiants; } set { _identifiants = value; } }
        public override bool UseDefaultCredentials
        {
            get { return _identifiants == CredentialCache.DefaultCredentials; }
            set { _identifiants = value ? CredentialCache.DefaultCredentials : null; }
        }

        public string UserAgent { get; set; }
        public string Accept { get; set; }
        public string Referer { get; set; }
        public override string ContentType { get; set; }
        public bool AllowAutoRedirect { get; set; }
        public int MaximumAutomaticRedirections { get; set; }
        public bool KeepAlive { get; set; }

        // Learn : AddRange(n) positif → « bytes=n- », négatif → « bytes=n » (les
        // n derniers octets) ; AddRange(de, à) → « bytes=de-à ».
        public void AddRange(int range) { AddRange((long)range); }
        public void AddRange(long range) { Plage(range >= 0 ? range + "-" : range.ToString()); }
        public void AddRange(int from, int to) { AddRange((long)from, (long)to); }

        public void AddRange(long from, long to)
        {
            if (from < 0 || to < 0 || from > to) throw new ArgumentOutOfRangeException("from");
            Plage(from + "-" + to);
        }

        private void Plage(string p) { _plage = _plage == null ? "bytes=" + p : _plage + "," + p; }

        public void AddHeader(string nom, string valeur) { _entetes.Add(nom + ": " + valeur); }

        public override void Abort()
        {
            Fermer();
        }

        internal void Fermer()
        {
            if (_requete != IntPtr.Zero) WinHttp.WinHttpCloseHandle(_requete);
            if (_connexion != IntPtr.Zero) WinHttp.WinHttpCloseHandle(_connexion);
            if (_session != IntPtr.Zero) WinHttp.WinHttpCloseHandle(_session);
            _requete = _connexion = _session = IntPtr.Zero;
        }

        public override WebResponse GetResponse()
        {
            if (_session != IntPtr.Zero) throw new InvalidOperationException("This operation cannot be performed after the request has been submitted.");
            // Mandataire : par défaut celui du système ; une WebProxy sans
            // adresse ou contournée : aucun ; sinon le mandataire nommé.
            int acces = 0;
            string nomMandataire = null;
            IWebProxy m = Proxy;
            if (_mandatairePose || m != WebRequest.DefaultWebProxy)
            {
                Uri via = m == null || m.IsBypassed(_uri) ? null : m.GetProxy(_uri);
                if (via == null || via == _uri) acces = 1; // WINHTTP_ACCESS_TYPE_NO_PROXY
                else { acces = 3; nomMandataire = via.Host + ":" + via.Port; }
            }
            string agent = string.IsNullOrEmpty(UserAgent) ? "Mozilla/4.0 (compatible; MSIE 6.0; MS Web Services Client Protocol)" : UserAgent;
            _session = WinHttp.WinHttpOpen(agent, acces, nomMandataire, null, 0);
            if (_session == IntPtr.Zero) throw WinHttp.Echec(Marshal.GetLastWin32Error());
            int delai = _delai < 0 ? 0 : _delai, lecture = _delaiLecture < 0 ? 0 : _delaiLecture;
            WinHttp.WinHttpSetTimeouts(_session, delai, delai, delai, Math.Max(delai, lecture));
            _connexion = WinHttp.WinHttpConnect(_session, _uri.DnsSafeHost, _uri.Port, 0);
            if (_connexion == IntPtr.Zero) { int e = Marshal.GetLastWin32Error(); Fermer(); throw WinHttp.Echec(e); }
            int drapeaux = _uri.Scheme == "https" ? 0x00800000 : 0; // WINHTTP_FLAG_SECURE
            _requete = WinHttp.WinHttpOpenRequest(_connexion, _methode, _uri.PathAndQuery, null, Referer, IntPtr.Zero, drapeaux);
            if (_requete == IntPtr.Zero) { int e = Marshal.GetLastWin32Error(); Fermer(); throw WinHttp.Echec(e); }
            System.Text.StringBuilder h = new System.Text.StringBuilder();
            if (_plage != null) h.Append("Range: ").Append(_plage).Append("\r\n");
            if (!string.IsNullOrEmpty(Accept)) h.Append("Accept: ").Append(Accept).Append("\r\n");
            if (!string.IsNullOrEmpty(ContentType)) h.Append("Content-Type: ").Append(ContentType).Append("\r\n");
            foreach (string e in _entetes) h.Append(e).Append("\r\n");
            if (h.Length > 0) WinHttp.WinHttpAddRequestHeaders(_requete, h.ToString(), -1, 0x20000000); // WINHTTP_ADDREQ_FLAG_ADD
            if (!WinHttp.WinHttpSendRequest(_requete, IntPtr.Zero, 0, IntPtr.Zero, 0, 0, IntPtr.Zero)
                || !WinHttp.WinHttpReceiveResponse(_requete, IntPtr.Zero))
            {
                int e = Marshal.GetLastWin32Error();
                Fermer();
                throw WinHttp.Echec(e);
            }
            HttpWebResponse r = new HttpWebResponse(this, _requete, _uri, _methode);
            // La réponse garde les poignées ; elle les fermera.
            if ((int)r.StatusCode >= 400)
            {
                throw new WebException("The remote server returned an error: (" + (int)r.StatusCode + ") " + r.StatusDescription + ".",
                    null, WebExceptionStatus.ProtocolError, r);
            }
            return r;
        }
    }

    public class HttpWebResponse : WebResponse
    {
        private readonly HttpWebRequest _demande;
        private readonly IntPtr _requete;
        private readonly Uri _uri;
        private readonly string _methode;
        private readonly HttpStatusCode _code;
        private readonly string _description;
        private readonly long _longueur;
        private Stream _flux;
        private bool _ferme;

        internal HttpWebResponse(HttpWebRequest demande, IntPtr requete, Uri uri, string methode)
        {
            _demande = demande;
            _requete = requete;
            _uri = uri;
            _methode = methode;
            int code;
            int.TryParse(WinHttp.Entete(requete, WinHttp.QUERY_STATUS_CODE, null), out code);
            _code = (HttpStatusCode)code;
            _description = WinHttp.Entete(requete, WinHttp.QUERY_STATUS_TEXT, null) ?? "";
            long l;
            string cl = WinHttp.Entete(requete, WinHttp.QUERY_CONTENT_LENGTH, null);
            _longueur = cl != null && long.TryParse(cl.Trim(), out l) ? l : -1;
        }

        public HttpStatusCode StatusCode { get { return _code; } }
        public string StatusDescription { get { return _description; } }
        public string Method { get { return _methode; } }
        public override Uri ResponseUri { get { return _uri; } }
        public override long ContentLength { get { return _longueur; } set { throw new NotSupportedException(); } }
        public override string ContentType { get { return GetResponseHeader("Content-Type"); } set { throw new NotSupportedException(); } }
        public string Server { get { return GetResponseHeader("Server"); } }

        // Learn : la valeur de l'en-tête, ou une chaîne vide s'il manque.
        public string GetResponseHeader(string headerName)
        {
            if (_ferme) throw new ObjectDisposedException("HttpWebResponse");
            return WinHttp.Entete(_requete, WinHttp.QUERY_CUSTOM, headerName) ?? "";
        }

        public override Stream GetResponseStream()
        {
            if (_ferme) throw new ObjectDisposedException("HttpWebResponse");
            if (_flux == null) _flux = new FluxWinHttp(this, _requete, _longueur);
            return _flux;
        }

        public override void Close()
        {
            if (_ferme) return;
            _ferme = true;
            _demande.Fermer();
        }
    }

    // Corps de la réponse, lu par WinHttpReadData.
    internal sealed class FluxWinHttp : Stream
    {
        private readonly HttpWebResponse _reponse;
        private readonly IntPtr _requete;
        private readonly long _longueur;
        private long _position;
        private bool _fini;

        public FluxWinHttp(HttpWebResponse reponse, IntPtr requete, long longueur)
        {
            _reponse = reponse;
            _requete = requete;
            _longueur = longueur;
        }

        public override bool CanRead { get { return true; } }
        public override bool CanSeek { get { return false; } }
        public override bool CanWrite { get { return false; } }
        public override long Length { get { throw new NotSupportedException(); } }
        public override long Position { get { return _position; } set { throw new NotSupportedException(); } }
        public override void Flush() { }
        public override long Seek(long offset, SeekOrigin origin) { throw new NotSupportedException(); }
        public override void SetLength(long value) { throw new NotSupportedException(); }
        public override void Write(byte[] buffer, int offset, int count) { throw new NotSupportedException(); }

        public override int Read(byte[] buffer, int offset, int count)
        {
            if (buffer == null) throw new ArgumentNullException("buffer");
            if (offset < 0 || count < 0 || offset + count > buffer.Length) throw new ArgumentOutOfRangeException("offset");
            if (count == 0 || _fini) return 0;
            IntPtr tampon = Marshal.AllocHGlobal(count);
            IntPtr lu = Marshal.AllocHGlobal(4);
            try
            {
                Marshal.WriteInt32(lu, 0);
                if (!WinHttp.WinHttpReadData(_requete, tampon, count, lu))
                    throw new IOException("Unable to read data from the transport connection.", WinHttp.Echec(Marshal.GetLastWin32Error()));
                int n = Marshal.ReadInt32(lu);
                if (n == 0) { _fini = true; return 0; }
                Marshal.Copy(tampon, buffer, offset, n);
                _position += n;
                return n;
            }
            finally
            {
                Marshal.FreeHGlobal(lu);
                Marshal.FreeHGlobal(tampon);
            }
        }

        protected override void Dispose(bool disposing)
        {
            _fini = true;
            _reponse.Close();
        }
    }
}
