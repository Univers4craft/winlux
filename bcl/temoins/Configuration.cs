// Témoin de ConfigurationManager dans la mscorlib Winlux (#2362).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 6) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait ConfigurationManager.GetSection.
// Le témoin écrit son propre <exe>.config avant la première lecture.
using System;
using System.Collections.Specialized;
using System.Configuration;
using System.IO;

namespace Temoins
{
    static class Configuration
    {
        const string Texte =
            "\uFEFF<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n" +
            "<!-- témoin -->\r\n" +
            "<configuration>\r\n" +
            "  <configSections>\r\n" +
            "    <section name=\"perso\" type=\"Perso.Handler, Perso\" />\r\n" +
            "  </configSections>\r\n" +
            "  <startup><supportedRuntime version=\"v4.0\" sku=\".NETFramework,Version=v4.5\" /></startup>\r\n" +
            "  <appSettings>\r\n" +
            "    <add key=\"Mode\" value=\"rapide\" />\r\n" +
            "    <add key='Chemin' value='C:\\a &amp; b &lt;c&gt; &#233;' />\r\n" +
            "    <add key=\"Vieux\" value=\"x\" /><remove key=\"vieux\" />\r\n" +
            "  </appSettings>\r\n" +
            "  <connectionStrings>\r\n" +
            "    <add name=\"Base\" connectionString=\"Data Source=x\" providerName=\"System.Data.SqlClient\" />\r\n" +
            "  </connectionStrings>\r\n" +
            "  <perso valeur=\"1\"><![CDATA[ <pas un élément> ]]></perso>\r\n" +
            "</configuration>\r\n";

        static int Main()
        {
            string chemin = AppDomain.CurrentDomain.SetupInformation.ConfigurationFile;
            if (chemin == null) return 1;
            File.WriteAllText(chemin, Texte);
            try { return Verifier(); }
            finally { File.Delete(chemin); }
        }

        static int Verifier()
        {
            // 1. appSettings : clés sans casse, entités décodées, remove.
            NameValueCollection a = ConfigurationManager.AppSettings;
            if (a == null || a.Count != 2 || a["mode"] != "rapide" || a["Chemin"] != "C:\\a & b <c> \u00e9" || a["Vieux"] != null) return 1;

            // 2. Lecture seule.
            try { a.Set("Mode", "lent"); return 2; } catch (NotSupportedException) { }
            if (ConfigurationManager.AppSettings != a) return 2;

            // 3. connectionStrings.
            ConnectionStringSettings c = ConfigurationManager.ConnectionStrings["base"];
            if (c == null || c.ConnectionString != "Data Source=x" || c.ProviderName != "System.Data.SqlClient") return 3;

            // 4. GetSection : sections connues, section absente.
            if (ConfigurationManager.GetSection("appSettings") != a) return 4;
            ConnectionStringsSection s = ConfigurationManager.GetSection("connectionStrings") as ConnectionStringsSection;
            if (s == null || s.ConnectionStrings["Base"] == null) return 4;
            if (ConfigurationManager.GetSection("absente") != null || ConfigurationManager.GetSection("groupe/absente") != null) return 4;

            // 5. Section présente avec un gestionnaire propre : refus explicite.
            try { ConfigurationManager.GetSection("perso"); return 5; }
            catch (ConfigurationErrorsException e) { if (e.Line != 16) return 5; }

            // 6. NameValueCollection seule : plusieurs valeurs par clé.
            NameValueCollection n = new NameValueCollection();
            n.Add("k", "1"); n.Add("K", "2"); n.Add("autre", null);
            if (n.Count != 2 || n["k"] != "1,2" || n.GetValues("k").Length != 2 || n.Get("autre") != null || n.AllKeys[1] != "autre") return 6;
            return 200;
        }
    }
}
