// System.Configuration.ConfigurationManager et NameValueCollection (#2362),
// d'après MS Learn : « ConfigurationManager Class », « GetSection(String) »,
// « AppSettings », « ConnectionStrings », « ConnectionStringSettings »,
// « appSettings Element », « connectionStrings Element »,
// « NameValueCollection Class », « ConfigurationErrorsException ».
// Le fichier lu est celui de l'application (<exe>.config,
// AppDomainSetup.ConfigurationFile). Corsair.Service.exe (iCUE) appelait
// ConfigurationManager.GetSection.
using System;
using System.Collections;
using System.Collections.Generic;
using System.Collections.Specialized;
using System.IO;
using System.Text;

namespace System.Collections.Specialized
{
    // Clés sans casse, plusieurs valeurs par clé, ordre d'ajout gardé.
    public class NameValueCollection : ICollection, IEnumerable
    {
        private readonly List<string> _cles = new List<string>();
        private readonly List<List<string>> _valeurs = new List<List<string>>();
        private bool _lectureSeule;

        public NameValueCollection() { }

        public NameValueCollection(NameValueCollection col)
        {
            if (col == null) throw new ArgumentNullException("col");
            Add(col);
        }

        internal void Figer() { _lectureSeule = true; }

        private void Modifiable()
        {
            if (_lectureSeule) throw new NotSupportedException("Collection is read-only.");
        }

        private int Indice(string name)
        {
            for (int i = 0; i < _cles.Count; i++)
            {
                string k = _cles[i];
                if (k == null ? name == null : name != null && string.Equals(k, name, StringComparison.OrdinalIgnoreCase))
                    return i;
            }
            return -1;
        }

        private static string Joindre(List<string> v)
        {
            if (v == null || v.Count == 0) return null;
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < v.Count; i++)
            {
                if (i > 0) sb.Append(',');
                sb.Append(v[i]);
            }
            return sb.ToString();
        }

        public int Count { get { return _cles.Count; } }
        public bool IsSynchronized { get { return false; } }
        public object SyncRoot { get { return this; } }

        public string[] AllKeys { get { return _cles.ToArray(); } }

        public virtual void Add(string name, string value)
        {
            Modifiable();
            int i = Indice(name);
            if (i < 0)
            {
                _cles.Add(name);
                List<string> l = new List<string>();
                if (value != null) l.Add(value);
                _valeurs.Add(l);
            }
            else if (value != null)
            {
                _valeurs[i].Add(value);
            }
        }

        public void Add(NameValueCollection c)
        {
            if (c == null) throw new ArgumentNullException("c");
            for (int i = 0; i < c._cles.Count; i++)
            {
                List<string> v = c._valeurs[i];
                if (v.Count == 0) Add(c._cles[i], null);
                foreach (string s in v) Add(c._cles[i], s);
            }
        }

        public virtual void Set(string name, string value)
        {
            Modifiable();
            int i = Indice(name);
            List<string> l = new List<string>();
            if (value != null) l.Add(value);
            if (i < 0) { _cles.Add(name); _valeurs.Add(l); }
            else _valeurs[i] = l;
        }

        public virtual void Remove(string name)
        {
            Modifiable();
            int i = Indice(name);
            if (i >= 0) { _cles.RemoveAt(i); _valeurs.RemoveAt(i); }
        }

        public virtual void Clear()
        {
            Modifiable();
            _cles.Clear();
            _valeurs.Clear();
        }

        public virtual string Get(string name)
        {
            int i = Indice(name);
            return i < 0 ? null : Joindre(_valeurs[i]);
        }

        public virtual string Get(int index) { return Joindre(_valeurs[index]); }
        public virtual string GetKey(int index) { return _cles[index]; }

        public virtual string[] GetValues(string name)
        {
            int i = Indice(name);
            return i < 0 || _valeurs[i].Count == 0 ? null : _valeurs[i].ToArray();
        }

        public virtual string[] GetValues(int index)
        {
            return _valeurs[index].Count == 0 ? null : _valeurs[index].ToArray();
        }

        public bool HasKeys() { return _cles.Count > 0; }

        public string this[string name] { get { return Get(name); } set { Set(name, value); } }
        public string this[int index] { get { return Get(index); } }

        public void CopyTo(Array dest, int index)
        {
            for (int i = 0; i < _cles.Count; i++) dest.SetValue(Get(i), index + i);
        }

        public IEnumerator GetEnumerator() { return _cles.GetEnumerator(); }
    }
}

namespace System.Configuration
{
    public class ConfigurationException : SystemException
    {
        public ConfigurationException() : base("An error occurred in the configuration system.") { }
        public ConfigurationException(string message) : base(message) { }
        public ConfigurationException(string message, Exception inner) : base(message, inner) { }
    }

    public class ConfigurationErrorsException : ConfigurationException
    {
        private readonly string _fichier;
        private readonly int _ligne;

        public ConfigurationErrorsException() { }
        public ConfigurationErrorsException(string message) : base(message) { }
        public ConfigurationErrorsException(string message, Exception inner) : base(message, inner) { }

        public ConfigurationErrorsException(string message, string filename, int line) : base(message)
        {
            _fichier = filename;
            _ligne = line;
        }

        public string Filename { get { return _fichier; } }
        public int Line { get { return _ligne; } }
    }

    public sealed class ConnectionStringSettings
    {
        public ConnectionStringSettings() { }
        public ConnectionStringSettings(string name, string connectionString) : this(name, connectionString, "") { }

        public ConnectionStringSettings(string name, string connectionString, string providerName)
        {
            Name = name;
            ConnectionString = connectionString;
            ProviderName = providerName;
        }

        public string Name { get; set; }
        public string ConnectionString { get; set; }
        public string ProviderName { get; set; }
        public override string ToString() { return ConnectionString; }
    }

    public sealed class ConnectionStringSettingsCollection : IEnumerable
    {
        private readonly List<ConnectionStringSettings> _liste = new List<ConnectionStringSettings>();

        public int Count { get { return _liste.Count; } }
        public ConnectionStringSettings this[int index] { get { return _liste[index]; } }

        public ConnectionStringSettings this[string name]
        {
            get
            {
                foreach (ConnectionStringSettings c in _liste)
                    if (string.Equals(c.Name, name, StringComparison.OrdinalIgnoreCase)) return c;
                return null;
            }
        }

        public void Add(ConnectionStringSettings settings)
        {
            if (settings == null) throw new ArgumentNullException("settings");
            Remove(settings.Name);
            _liste.Add(settings);
        }

        public void Remove(string name)
        {
            for (int i = _liste.Count - 1; i >= 0; i--)
                if (string.Equals(_liste[i].Name, name, StringComparison.OrdinalIgnoreCase)) _liste.RemoveAt(i);
        }

        public void Clear() { _liste.Clear(); }
        public IEnumerator GetEnumerator() { return _liste.GetEnumerator(); }
    }

    public sealed class ConnectionStringsSection
    {
        private readonly ConnectionStringSettingsCollection _chaines;
        internal ConnectionStringsSection(ConnectionStringSettingsCollection c) { _chaines = c; }
        public ConnectionStringSettingsCollection ConnectionStrings { get { return _chaines; } }
    }

    public static class ConfigurationManager
    {
        private static readonly object s_verrou = new object();
        private static bool s_lu;
        private static Element s_racine;
        private static string s_fichier;
        private static NameValueCollection s_appSettings;
        private static ConnectionStringSettingsCollection s_chaines;

        public static NameValueCollection AppSettings
        {
            get { Lire(); return s_appSettings; }
        }

        public static ConnectionStringSettingsCollection ConnectionStrings
        {
            get { Lire(); return s_chaines; }
        }

        // La section nommée, ou null si le fichier ne la contient pas.
        // « appSettings » et « connectionStrings » sont servies ici ; une
        // section déclarée dans <configSections> avec son propre gestionnaire
        // demanderait System.Xml, absent de notre mscorlib : refus explicite.
        public static object GetSection(string sectionName)
        {
            if (sectionName == null) throw new ArgumentNullException("sectionName");
            Lire();
            if (string.Equals(sectionName, "appSettings", StringComparison.Ordinal)) return s_appSettings;
            if (string.Equals(sectionName, "connectionStrings", StringComparison.Ordinal)) return new ConnectionStringsSection(s_chaines);
            Element e = Trouver(sectionName);
            if (e == null) return null;
            throw new ConfigurationErrorsException(
                "The configuration section '" + sectionName + "' has a handler that Winlux cannot run yet (System.Xml is not available).",
                s_fichier, e.Ligne);
        }

        public static void RefreshSection(string sectionName)
        {
            lock (s_verrou) { s_lu = false; }
        }

        private static void Lire()
        {
            lock (s_verrou)
            {
                if (s_lu) return;
                s_lu = true;
                s_racine = null;
                s_appSettings = new NameValueCollection();
                s_chaines = new ConnectionStringSettingsCollection();
                s_fichier = AppDomain.CurrentDomain.SetupInformation.ConfigurationFile;
                if (s_fichier == null || !File.Exists(s_fichier)) { s_appSettings.Figer(); return; }
                s_racine = Xml.Analyser(File.ReadAllText(s_fichier), s_fichier);
                if (s_racine == null || s_racine.Nom != "configuration")
                    throw new ConfigurationErrorsException("Configuration system failed to initialize: the root element must be <configuration>.", s_fichier, 1);
                Element app = s_racine.Enfant("appSettings");
                if (app != null)
                {
                    string externe = app.Attribut("file");
                    if (!string.IsNullOrEmpty(externe))
                    {
                        string chemin = Path.Combine(Path.GetDirectoryName(s_fichier), externe);
                        if (File.Exists(chemin))
                        {
                            Element ext = Xml.Analyser(File.ReadAllText(chemin), chemin);
                            if (ext != null) Paires(ext, s_appSettings);
                        }
                    }
                    Paires(app, s_appSettings);
                }
                Element cs = s_racine.Enfant("connectionStrings");
                if (cs != null)
                {
                    foreach (Element c in cs.Enfants)
                    {
                        if (c.Nom == "add")
                            s_chaines.Add(new ConnectionStringSettings(c.Attribut("name"), c.Attribut("connectionString"), c.Attribut("providerName") ?? ""));
                        else if (c.Nom == "remove") s_chaines.Remove(c.Attribut("name"));
                        else if (c.Nom == "clear") s_chaines.Clear();
                    }
                }
                s_appSettings.Figer();
            }
        }

        private static void Paires(Element section, NameValueCollection dest)
        {
            foreach (Element c in section.Enfants)
            {
                if (c.Nom == "add") dest.Set(c.Attribut("key"), c.Attribut("value"));
                else if (c.Nom == "remove") dest.Remove(c.Attribut("key"));
                else if (c.Nom == "clear") dest.Clear();
            }
        }

        // « groupe/section » : chemin d'éléments sous <configuration>.
        private static Element Trouver(string chemin)
        {
            if (s_racine == null) return null;
            Element e = s_racine;
            foreach (string part in chemin.Split('/'))
            {
                e = e.Enfant(part);
                if (e == null) return null;
            }
            return e;
        }
    }

    // Élément XML d'un fichier de configuration : nom, attributs, enfants.
    internal sealed class Element
    {
        public string Nom;
        public int Ligne;
        public readonly List<string> NomsAttributs = new List<string>();
        public readonly List<string> ValeursAttributs = new List<string>();
        public readonly List<Element> Enfants = new List<Element>();

        public string Attribut(string nom)
        {
            for (int i = 0; i < NomsAttributs.Count; i++)
                if (NomsAttributs[i] == nom) return ValeursAttributs[i];
            return null;
        }

        public Element Enfant(string nom)
        {
            foreach (Element e in Enfants)
                if (e.Nom == nom) return e;
            return null;
        }
    }

    // Lecteur XML minimal : déclaration, commentaires, CDATA, instructions,
    // DOCTYPE sans sous-ensemble interne, entités prédéfinies et numériques.
    // Le texte des éléments est ignoré (les sections lues n'en ont pas).
    internal sealed class Xml
    {
        private readonly string _t;
        private readonly string _fichier;
        private int _p;

        private Xml(string texte, string fichier) { _t = texte; _fichier = fichier; }

        public static Element Analyser(string texte, string fichier)
        {
            Xml x = new Xml(texte, fichier);
            if (x._t.Length > 0 && x._t[0] == '﻿') x._p = 1;
            Element racine = null;
            while (true)
            {
                x.Blancs();
                if (x._p >= x._t.Length) break;
                if (x.Commence("<?")) { x.Jusqua("?>"); continue; }
                if (x.Commence("<!--")) { x.Jusqua("-->"); continue; }
                if (x.Commence("<!")) { x.Jusqua(">"); continue; }
                if (x._t[x._p] != '<') throw x.Erreur("Data at the root level is invalid.");
                if (racine != null) throw x.Erreur("There are multiple root elements.");
                racine = x.LireElement();
            }
            return racine;
        }

        private ConfigurationErrorsException Erreur(string message)
        {
            return new ConfigurationErrorsException(message, _fichier, LigneDe(_p));
        }

        private int LigneDe(int pos)
        {
            int n = 1;
            for (int i = 0; i < pos && i < _t.Length; i++) if (_t[i] == '\n') n++;
            return n;
        }

        private bool Commence(string s)
        {
            if (_p + s.Length > _t.Length) return false;
            for (int i = 0; i < s.Length; i++)
                if (_t[_p + i] != s[i]) return false;
            return true;
        }

        private void Jusqua(string fin)
        {
            int i = _t.IndexOf(fin, _p, StringComparison.Ordinal);
            if (i < 0) throw Erreur("Unexpected end of file.");
            _p = i + fin.Length;
        }

        private void Blancs()
        {
            while (_p < _t.Length && (_t[_p] == ' ' || _t[_p] == '\t' || _t[_p] == '\r' || _t[_p] == '\n')) _p++;
        }

        private static bool CarNom(char c)
        {
            return char.IsLetterOrDigit(c) || c == '_' || c == ':' || c == '-' || c == '.';
        }

        private string Nom()
        {
            int d = _p;
            while (_p < _t.Length && CarNom(_t[_p])) _p++;
            if (_p == d) throw Erreur("Name expected.");
            return _t.Substring(d, _p - d);
        }

        private Element LireElement()
        {
            Element e = new Element();
            e.Ligne = LigneDe(_p);
            _p++;
            e.Nom = Nom();
            while (true)
            {
                Blancs();
                if (_p >= _t.Length) throw Erreur("Unexpected end of file.");
                if (Commence("/>")) { _p += 2; return e; }
                if (_t[_p] == '>') { _p++; break; }
                string n = Nom();
                Blancs();
                if (_p >= _t.Length || _t[_p] != '=') throw Erreur("'=' expected.");
                _p++;
                Blancs();
                if (_p >= _t.Length || (_t[_p] != '"' && _t[_p] != '\'')) throw Erreur("Quote expected.");
                char q = _t[_p++];
                int fin = _t.IndexOf(q, _p);
                if (fin < 0) throw Erreur("Unexpected end of file.");
                e.NomsAttributs.Add(n);
                e.ValeursAttributs.Add(Entites(_t.Substring(_p, fin - _p)));
                _p = fin + 1;
            }
            while (true)
            {
                int lt = _t.IndexOf('<', _p);
                if (lt < 0) throw Erreur("Unexpected end of file.");
                _p = lt;
                if (Commence("</"))
                {
                    _p += 2;
                    string n = Nom();
                    if (n != e.Nom) throw Erreur("The '" + e.Nom + "' start tag does not match the end tag of '" + n + "'.");
                    Blancs();
                    if (_p >= _t.Length || _t[_p] != '>') throw Erreur("'>' expected.");
                    _p++;
                    return e;
                }
                if (Commence("<!--")) { Jusqua("-->"); continue; }
                if (Commence("<![CDATA[")) { Jusqua("]]>"); continue; }
                if (Commence("<?")) { Jusqua("?>"); continue; }
                e.Enfants.Add(LireElement());
            }
        }

        private string Entites(string s)
        {
            if (s.IndexOf('&') < 0) return s;
            StringBuilder sb = new StringBuilder();
            int i = 0;
            while (i < s.Length)
            {
                char c = s[i];
                if (c != '&') { sb.Append(c); i++; continue; }
                int fin = s.IndexOf(';', i);
                if (fin < 0) throw Erreur("An error occurred while parsing EntityName.");
                string nom = s.Substring(i + 1, fin - i - 1);
                if (nom == "lt") sb.Append('<');
                else if (nom == "gt") sb.Append('>');
                else if (nom == "amp") sb.Append('&');
                else if (nom == "quot") sb.Append('"');
                else if (nom == "apos") sb.Append('\'');
                else if (nom.Length > 2 && nom[0] == '#' && (nom[1] == 'x' || nom[1] == 'X'))
                    sb.Append((char)Convert.ToInt32(nom.Substring(2), 16));
                else if (nom.Length > 1 && nom[0] == '#')
                    sb.Append((char)int.Parse(nom.Substring(1)));
                else throw Erreur("Reference to undeclared entity '" + nom + "'.");
                i = fin + 1;
            }
            return sb.ToString();
        }
    }
}
