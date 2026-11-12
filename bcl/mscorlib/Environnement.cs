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

        [MethodImpl(MethodImplOptions.InternalCall)]
        public static extern void Exit(int exitCode);

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

        [MethodImpl(MethodImplOptions.InternalCall)]
        private static extern string CheminSpecial(int csidl);

        // --- Géré ---

        public static string GetEnvironmentVariable(string variable, EnvironmentVariableTarget target)
        {
            if (variable == null) throw new ArgumentNullException("variable");
            return target == EnvironmentVariableTarget.Process ? GetEnvironmentVariable(variable) : null;
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
