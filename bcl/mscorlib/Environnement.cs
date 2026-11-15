// System.Environment de la mscorlib Winlux (#1991), OperatingSystem et
// PlatformID.
// Source : la documentation publique de .NET Framework 4.x (MS Learn) pour
// les signatures et les valeurs des énumérations ; les valeurs de
// SpecialFolder sont les CSIDL publics de shlobj.h (mingw-w64).
// Les membres « extern » sont servis par le moteur (bcl.rs, threads.rs).
using System.Runtime.CompilerServices;

namespace System
{
    public enum PlatformID
    {
        Win32S = 0,
        Win32Windows = 1,
        Win32NT = 2,
        WinCE = 3,
        Unix = 4,
        Xbox = 5,
        MacOSX = 6,
    }

    public enum EnvironmentVariableTarget
    {
        Process = 0,
        User = 1,
        Machine = 2,
    }

    public sealed class OperatingSystem
    {
        private readonly PlatformID _platform;
        private readonly Version _version;
        private readonly string _servicePack;

        public OperatingSystem(PlatformID platform, Version version) : this(platform, version, string.Empty) { }

        internal OperatingSystem(PlatformID platform, Version version, string servicePack)
        {
            if (version == null) throw new ArgumentNullException("version");
            _platform = platform;
            _version = version;
            _servicePack = servicePack;
        }

        public PlatformID Platform { get { return _platform; } }
        public Version Version { get { return _version; } }
        public string ServicePack { get { return _servicePack; } }

        public string VersionString
        {
            get
            {
                string nom = _platform == PlatformID.Win32NT ? "Microsoft Windows NT " : _platform == PlatformID.Unix ? "Unix " : "<unknown> ";
                string texte = nom + _version.ToString();
                return _servicePack.Length == 0 ? texte : texte + " " + _servicePack;
            }
        }

        public override string ToString() { return VersionString; }
    }

    public static class Environment
    {
        public enum SpecialFolder
        {
            Desktop = 0,
            Programs = 2,
            MyDocuments = 5,
            Personal = 5,
            Favorites = 6,
            Startup = 7,
            Recent = 8,
            SendTo = 9,
            StartMenu = 11,
            MyMusic = 13,
            MyVideos = 14,
            DesktopDirectory = 16,
            MyComputer = 17,
            NetworkShortcuts = 19,
            Fonts = 20,
            Templates = 21,
            CommonStartMenu = 22,
            CommonPrograms = 23,
            CommonStartup = 24,
            CommonDesktopDirectory = 25,
            ApplicationData = 26,
            PrinterShortcuts = 27,
            LocalApplicationData = 28,
            InternetCache = 32,
            Cookies = 33,
            History = 34,
            CommonApplicationData = 35,
            Windows = 36,
            System = 37,
            ProgramFiles = 38,
            MyPictures = 39,
            UserProfile = 40,
            SystemX86 = 41,
            ProgramFilesX86 = 42,
            CommonProgramFiles = 43,
            CommonProgramFilesX86 = 44,
            CommonTemplates = 45,
            CommonDocuments = 46,
            CommonAdminTools = 47,
            AdminTools = 48,
            CommonMusic = 53,
            CommonPictures = 54,
            CommonVideos = 55,
            Resources = 56,
            LocalizedResources = 57,
            CommonOemLinks = 58,
            CDBurning = 59,
        }

        public enum SpecialFolderOption
        {
            None = 0,
            DoNotVerify = 0x4000,
            Create = 0x8000,
        }

        // --- Servi par le moteur ---

        // ProcessExit est levé avant la sortie (#2058).
        public static void Exit(int exitCode)
        {
            AppDomain.LeverSortie();
            Quitter(exitCode);
        }

        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern void Quitter(int exitCode);

        public static extern int ExitCode
        {
            [MethodImpl(MethodImplOptions.InternalCall)] get;
            [MethodImpl(MethodImplOptions.InternalCall)] set;
        }

        public static extern string CurrentDirectory { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern string MachineName { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern string UserName { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern string NewLine { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern int TickCount { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern long TickCount64 { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern int ProcessorCount { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern bool Is64BitProcess { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern bool Is64BitOperatingSystem { [MethodImpl(MethodImplOptions.InternalCall)] get; }
        public static extern int CurrentManagedThreadId { [MethodImpl(MethodImplOptions.InternalCall)] get; }

        // Faux dans un processus lancé comme service Windows, comme sous
        // Windows où un service n'a pas de station de fenêtres visible.
        public static extern bool UserInteractive { [MethodImpl(MethodImplOptions.InternalCall)] get; }

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern string GetEnvironmentVariable(string variable);

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern string[] GetCommandLineArgs();

        // Le bloc d'environnement du processus, « NOM=valeur » par entrée,
        // comme GetEnvironmentStringsW (#2233).
        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern string[] BlocEnvironnement();

        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern string CheminSpecial(int csidl);

        // --- Géré ---

        public static string GetEnvironmentVariable(string variable, EnvironmentVariableTarget target)
        {
            if (variable == null) throw new ArgumentNullException("variable");
            return target == EnvironmentVariableTarget.Process ? GetEnvironmentVariable(variable) : null;
        }

        // Toutes les variables du processus, dans une Hashtable (#2233).
        public static System.Collections.IDictionary GetEnvironmentVariables()
        {
            System.Collections.Hashtable table = new System.Collections.Hashtable();
            string[] bloc = BlocEnvironnement();
            if (bloc == null) return table;
            for (int i = 0; i < bloc.Length; i++)
            {
                string entree = bloc[i];
                // Un nom peut commencer par « = » (=C:) : le séparateur est
                // le premier « = » après le premier caractère.
                int egal = entree == null || entree.Length < 2 ? -1 : entree.IndexOf('=', 1);
                if (egal > 0) table[entree.Substring(0, egal)] = entree.Substring(egal + 1);
            }
            return table;
        }

        // User et Machine se lisent dans le registre, aux clés que Windows
        // documente pour ces variables.
        public static System.Collections.IDictionary GetEnvironmentVariables(EnvironmentVariableTarget target)
        {
            if (target == EnvironmentVariableTarget.Process) return GetEnvironmentVariables();
            Microsoft.Win32.RegistryKey racine;
            string chemin;
            if (target == EnvironmentVariableTarget.User)
            {
                racine = Microsoft.Win32.Registry.CurrentUser;
                chemin = "Environment";
            }
            else if (target == EnvironmentVariableTarget.Machine)
            {
                racine = Microsoft.Win32.Registry.LocalMachine;
                chemin = "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment";
            }
            else
            {
                throw new ArgumentException("Enum value was out of legal range.", "target");
            }
            System.Collections.Hashtable table = new System.Collections.Hashtable();
            Microsoft.Win32.RegistryKey cle = racine.OpenSubKey(chemin, false);
            if (cle == null) return table;
            string[] noms = cle.GetValueNames();
            for (int i = 0; i < noms.Length; i++)
            {
                object v = cle.GetValue(noms[i]);
                if (v != null) table[noms[i]] = v.ToString();
            }
            cle.Close();
            return table;
        }

        // %NOM% remplacé par la variable d'environnement ; un nom inconnu est
        // laissé tel quel, comme ExpandEnvironmentStrings (#2020).
        public static string ExpandEnvironmentVariables(string name)
        {
            if (name == null) throw new ArgumentNullException("name");
            System.Text.StringBuilder sb = new System.Text.StringBuilder();
            int i = 0;
            while (i < name.Length)
            {
                int debut = name.IndexOf('%', i);
                if (debut < 0) { sb.Append(name, i, name.Length - i); break; }
                int fin = name.IndexOf('%', debut + 1);
                if (fin < 0) { sb.Append(name, i, name.Length - i); break; }
                sb.Append(name, i, debut - i);
                string valeur = fin > debut + 1 ? GetEnvironmentVariable(name.Substring(debut + 1, fin - debut - 1)) : null;
                if (valeur != null) { sb.Append(valeur); i = fin + 1; }
                else { sb.Append(name, debut, fin - debut); i = fin; }
            }
            return sb.ToString();
        }

        public static string GetFolderPath(SpecialFolder folder) { return GetFolderPath(folder, SpecialFolderOption.None); }

        public static string GetFolderPath(SpecialFolder folder, SpecialFolderOption option)
        {
            return CheminSpecial((int)folder | (option == SpecialFolderOption.Create ? 0x8000 : 0)) ?? string.Empty;
        }

        public static string SystemDirectory { get { return GetFolderPath(SpecialFolder.System); } }

        // Même version que GetVersionEx de kernel32.
        public static OperatingSystem OSVersion
        {
            get { return new OperatingSystem(PlatformID.Win32NT, new Version(10, 0, 19045, 0)); }
        }

        // Version du CLR de .NET Framework 4.8.
        public static Version Version { get { return new Version(4, 0, 30319, 42000); } }

        public static bool HasShutdownStarted { get { return false; } }
    }
}
