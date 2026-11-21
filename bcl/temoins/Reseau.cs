// Témoin de System.Net.HttpWebRequest sur notre WinHTTP (#2814).
// Façade de compilation : System.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// Le téléchargeur de Paint.NET (SetupDownloader) lit son manifeste puis le
// paquet avec WebRequest.Create, AddRange pour reprendre, Proxy et
// GetResponseStream : notre mscorlib n'avait aucun de ces types.
//
// WINLUX_TEMOIN_URL donne l'adresse d'un serveur local (scripts/
// test-webrequest.sh) : /donnees sert 4096 octets (i & 0xFF), /absent 404 ;
// le même serveur relaie une URL absolue comme un mandataire et marque sa
// réponse « X-Mandataire: oui ». Sans elle, seules les vérifications hors
// réseau tournent.
extern alias Facade;
using System;
using System.IO;
using Uri = Facade::System.Uri;
using UriFormatException = Facade::System.UriFormatException;
using WebRequest = Facade::System.Net.WebRequest;
using HttpWebRequest = Facade::System.Net.HttpWebRequest;
using HttpWebResponse = Facade::System.Net.HttpWebResponse;
using WebException = Facade::System.Net.WebException;
using WebExceptionStatus = Facade::System.Net.WebExceptionStatus;
using HttpStatusCode = Facade::System.Net.HttpStatusCode;
using WebProxy = Facade::System.Net.WebProxy;
using CredentialCache = Facade::System.Net.CredentialCache;

static class Reseau
{
    static byte[] Lire(Stream s)
    {
        MemoryStream m = new MemoryStream();
        byte[] b = new byte[700]; // pas un diviseur de 4096 : plusieurs lectures partielles
        int n;
        while ((n = s.Read(b, 0, b.Length)) > 0) m.Write(b, 0, n);
        return m.ToArray();
    }

    static bool Motif(byte[] d, int debut, int longueur)
    {
        if (d.Length != longueur) return false;
        for (int i = 0; i < d.Length; i++) if (d[i] != (byte)((debut + i) & 0xFF)) return false;
        return true;
    }

    static int Main()
    {
        // 1. Uri découpe une adresse absolue.
        Uri u = new Uri("https://download.example.org:8443/paint/net.json?v=5#x");
        if (u.Scheme != "https" || u.Host != "download.example.org" || u.Port != 8443
            || u.PathAndQuery != "/paint/net.json?v=5") return 1;
        if (new Uri("http://exemple.test/").Port != 80) return 1;
        try { new Uri("pas une adresse"); return 1; } catch (UriFormatException) { }

        // 2. Create rend une HttpWebRequest pour http, refuse un schéma inconnu.
        if (!(WebRequest.Create("http://exemple.test/a") is HttpWebRequest)) return 2;
        try { WebRequest.Create("gopher://exemple.test/"); return 2; } catch (NotSupportedException) { }
        if (CredentialCache.DefaultCredentials == null) return 2;

        string base_ = Environment.GetEnvironmentVariable("WINLUX_TEMOIN_URL");
        if (string.IsNullOrEmpty(base_))
        {
            Console.WriteLine("Reseau : WINLUX_TEMOIN_URL absente, vérifications réseau sautées.");
            return 200;
        }

        // 3. GET entier : octets exacts et ContentLength.
        HttpWebRequest q = (HttpWebRequest)WebRequest.Create(base_ + "/donnees");
        q.UserAgent = "TemoinWinlux/1.0";
        using (HttpWebResponse r = (HttpWebResponse)q.GetResponse())
        {
            if (r.StatusCode != HttpStatusCode.OK || r.ContentLength != 4096) return 3;
            if (!Motif(Lire(r.GetResponseStream()), 0, 4096)) return 3;
            if (r.GetResponseHeader("X-Agent") != "TemoinWinlux/1.0") return 3;
        }

        // 4. AddRange(1000) : reprise, 206 et les octets à partir de 1000.
        q = (HttpWebRequest)WebRequest.Create(base_ + "/donnees");
        q.AddRange(1000);
        using (HttpWebResponse r = (HttpWebResponse)q.GetResponse())
        {
            if (r.StatusCode != HttpStatusCode.PartialContent || r.ContentLength != 3096) return 4;
            if (!Motif(Lire(r.GetResponseStream()), 1000, 3096)) return 4;
        }

        // 5. AddRange(de, à) : plage fermée.
        q = (HttpWebRequest)WebRequest.Create(base_ + "/donnees");
        q.AddRange(10L, 19L);
        using (HttpWebResponse r = (HttpWebResponse)q.GetResponse())
            if (!Motif(Lire(r.GetResponseStream()), 10, 10)) return 5;

        // 6. 404 : WebException ProtocolError qui porte la réponse.
        try
        {
            WebRequest.Create(base_ + "/absent").GetResponse();
            return 6;
        }
        catch (WebException e)
        {
            HttpWebResponse r = e.Response as HttpWebResponse;
            if (e.Status != WebExceptionStatus.ProtocolError || r == null || r.StatusCode != HttpStatusCode.NotFound) return 6;
            r.Close();
        }

        // 7. new WebProxy() : sans adresse, la requête part en direct.
        q = (HttpWebRequest)WebRequest.Create(base_ + "/donnees");
        q.Proxy = new WebProxy();
        using (HttpWebResponse r = (HttpWebResponse)q.GetResponse())
            if (r.GetResponseHeader("X-Mandataire") != "") return 7;

        // 8. Un mandataire nommé reçoit l'URL absolue d'un hôte qu'il est
        //    seul à connaître.
        q = (HttpWebRequest)WebRequest.Create("http://paquets.winlux.invalid/donnees");
        q.Proxy = new WebProxy(base_);
        using (HttpWebResponse r = (HttpWebResponse)q.GetResponse())
        {
            if (r.GetResponseHeader("X-Mandataire") != "oui") return 8;
            if (!Motif(Lire(r.GetResponseStream()), 0, 4096)) return 8;
        }

        // 9. Personne n'écoute : WebException ConnectFailure, pas de plantage.
        q = (HttpWebRequest)WebRequest.Create("http://127.0.0.1:9/");
        q.Proxy = new WebProxy();
        q.Timeout = 5000;
        try { q.GetResponse(); return 9; }
        catch (WebException e) { if (e.Status != WebExceptionStatus.ConnectFailure) return 9; }

        return 200;
    }
}
