// System.AppDomain de la mscorlib Winlux (#2054). Licence GPL 3.
// Écrit d'après la documentation publique de .NET Framework 4.x (MS Learn :
// « AppDomain Class », « AppDomainSetup », « UnhandledExceptionEventArgs »,
// « ResolveEventArgs ») ; aucun code de Microsoft, de Mono ou de Wine.
//
// Winlux n'a qu'un domaine par processus, le domaine par défaut.
// AssemblyResolve est consulté par Assembly.Load quand une assembly manque ;
// les autres événements se mémorisent, mais le moteur ne lève pas encore
// ProcessExit ni UnhandledException (#2058), levés depuis par le moteur.
using System.Collections.Generic;
using System.IO;
using System.Reflection;

namespace System
{
    public class UnhandledExceptionEventArgs : EventArgs
    {
        private readonly object _exception;
        private readonly bool _fin;
        public UnhandledExceptionEventArgs(object exception, bool isTerminating) { _exception = exception; _fin = isTerminating; }
        public object ExceptionObject { get { return _exception; } }
        public bool IsTerminating { get { return _fin; } }
    }

    public delegate void UnhandledExceptionEventHandler(object sender, UnhandledExceptionEventArgs e);

    public class ResolveEventArgs : EventArgs
    {
        private readonly string _nom;
        private readonly Assembly _demandeur;
        public ResolveEventArgs(string name) { _nom = name; }
        public ResolveEventArgs(string name, Assembly requestingAssembly) { _nom = name; _demandeur = requestingAssembly; }
        public string Name { get { return _nom; } }
        public Assembly RequestingAssembly { get { return _demandeur; } }
    }

    public delegate Assembly ResolveEventHandler(object sender, ResolveEventArgs args);

    public sealed class AppDomainSetup
    {
        public string ApplicationBase { get; set; }
        public string ApplicationName { get; set; }
        public string ConfigurationFile { get; set; }
        public string PrivateBinPath { get; set; }
        public string ShadowCopyFiles { get; set; }
        public string TargetFrameworkName { get; set; }
    }

    public sealed class AppDomain
    {
        private static AppDomain _courant;
        private readonly string _base;
        private readonly string _nom;
        private readonly AppDomainSetup _setup;
        private readonly Dictionary<string, object> _donnees = new Dictionary<string, object>();

        private AppDomain()
        {
            Assembly entree = Assembly.GetEntryAssembly();
            string chemin = entree == null ? "" : entree.Location;
            string dossier = chemin.Length == 0 ? Directory.GetCurrentDirectory() : Path.GetDirectoryName(chemin);
            if (!dossier.EndsWith("\\")) dossier += "\\";
            _base = dossier;
            _nom = chemin.Length == 0 ? "DefaultDomain" : Path.GetFileName(chemin);
            _setup = new AppDomainSetup();
            _setup.ApplicationBase = _base;
            _setup.ApplicationName = _nom;
            _setup.ConfigurationFile = chemin.Length == 0 ? null : chemin + ".config";
        }

        public static AppDomain CurrentDomain
        {
            get
            {
                if (_courant == null) _courant = new AppDomain();
                return _courant;
            }
        }

        public string BaseDirectory { get { return _base; } }
        public string FriendlyName { get { return _nom; } }
        public int Id { get { return 1; } }
        public bool IsDefaultAppDomain() { return true; }
        public AppDomainSetup SetupInformation { get { return _setup; } }
        public string RelativeSearchPath { get { return _setup.PrivateBinPath; } }
        public string DynamicDirectory { get { return null; } }
        public bool ShadowCopyFiles { get { return false; } }
        public bool IsFullyTrusted { get { return true; } }
        public bool IsHomogenous { get { return true; } }

        public Assembly[] GetAssemblies() { return Rtti.Assemblies(); }

        public Assembly Load(string assemblyString) { return Assembly.Load(assemblyString); }

        public object GetData(string name)
        {
            if (name == null) throw new ArgumentNullException("name");
            switch (name)
            {
                case "APPBASE": return _base;
                case "APP_NAME": return _nom;
                case "APP_CONFIG_FILE": return _setup.ConfigurationFile;
            }
            object v;
            return _donnees.TryGetValue(name, out v) ? v : null;
        }

        public void SetData(string name, object data)
        {
            if (name == null) throw new ArgumentNullException("name");
            _donnees[name] = data;
        }

#pragma warning disable 67
        public event UnhandledExceptionEventHandler UnhandledException;
        public event EventHandler ProcessExit;
        public event EventHandler DomainUnload;
        public event ResolveEventHandler AssemblyResolve;
        public event ResolveEventHandler TypeResolve;
        public event ResolveEventHandler ResourceResolve;
        public event ResolveEventHandler ReflectionOnlyAssemblyResolve;
#pragma warning restore 67

        // Gestionnaires d'AssemblyResolve : un délégué multiple rend le
        // résultat du dernier (GetInvocationList manque encore).
        internal Assembly Resoudre(string nom, Assembly demandeur)
        {
            ResolveEventHandler h = AssemblyResolve;
            return h == null ? null : h(this, new ResolveEventArgs(nom, demandeur));
        }

        // Appelé par le moteur au retour de Main et par Environment.Exit
        // (#2058). Les gestionnaires ne sont levés qu'une fois, même si l'un
        // d'eux appelle Environment.Exit. Sans CurrentDomain lu, personne
        // n'a pu s'abonner.
        internal static void LeverSortie()
        {
            AppDomain d = _courant;
            if (d == null) return;
            EventHandler h = d.ProcessExit;
            d.ProcessExit = null;
            if (h != null) h(d, EventArgs.Empty);
        }

        // Appelé par le moteur avant l'arrêt sur une exception non gérée
        // (#2058) : IsTerminating vaut true.
        internal static void LeverExceptionNonGeree(object exception)
        {
            AppDomain d = _courant;
            if (d == null) return;
            UnhandledExceptionEventHandler h = d.UnhandledException;
            d.UnhandledException = null;
            if (h != null) h(d, new UnhandledExceptionEventArgs(exception, true));
        }

        public override string ToString() { return "Name:" + _nom + "\nThere are no context policies.\n"; }
    }
}
