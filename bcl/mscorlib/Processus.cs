// System.Diagnostics.Process de la mscorlib Winlux (#2050).
// Licence GPL 3. Écrit d'après la documentation publique de .NET Framework
// 4.x (MS Learn : « Process Class », « ProcessStartInfo Class »,
// « Win32Exception Class ») ; aucun code de Microsoft, de Mono ou de Wine.
//
// Les processus passent par les primitives de ProcessusNatif
// (clr/processus.rs), qui appellent kernel32 : un programme .NET voit les
// mêmes processus, noms et codes de sortie qu'un programme natif. Comme sous
// Windows, le processus d'un autre compte se liste et se nomme, mais ne
// s'ouvre pas (accès refusé) ; un zombie est un processus terminé (#1914).
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

namespace System.Runtime.InteropServices
{
    public class ExternalException : SystemException
    {
        public ExternalException() : base("External component has thrown an exception.") { }
        public ExternalException(string message) : base(message) { }
        private readonly int _hresult = unchecked((int)0x80004005);
        public ExternalException(string message, int errorCode) : base(message) { _hresult = errorCode; }
        public virtual int ErrorCode { get { return _hresult; } }
    }
}

namespace System.ComponentModel
{
    public class Win32Exception : ExternalException
    {
        private readonly int _code;

        // Sans code : la dernière erreur Win32, que notre mscorlib ne garde
        // pas encore (Marshal.GetLastWin32Error rend 0).
        public Win32Exception() : this(0) { }
        public Win32Exception(int error) : this(error, Texte(error)) { }
        public Win32Exception(int error, string message) : base(message, unchecked((int)0x80004005)) { _code = error; }
        public Win32Exception(string message) : this(0, message) { }

        public int NativeErrorCode { get { return _code; } }

        internal static string Texte(int error)
        {
            switch (error)
            {
                case 2: return "The system cannot find the file specified.";
                case 3: return "The system cannot find the path specified.";
                case 5: return "Access is denied.";
                case 6: return "The handle is invalid.";
                case 87: return "The parameter is incorrect.";
                case 193: return "The specified executable is not a valid application for this OS platform.";
                case 0: return "The operation completed successfully.";
                case 1155: return "No application is associated with the specified file for this operation.";
                default: return "Unknown error (0x" + error.ToString("x") + ")";
            }
        }
    }
}

namespace System.Diagnostics
{
    // Primitives du moteur (processus.rs). Un échec rend -code Win32.
    internal static class ProcessusNatif
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Courant();
        // [pid, nombre de fils, début]… à plat, zombies exclus.
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long[] Liste();
        // Nom Windows (« iCUE.exe »), ou null si le processus n'existe pas.
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string Nom(int pid);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long Ouvrir(int pid);
        // 259 (STILL_ACTIVE) tant qu'il tourne.
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long CodeSortie(long h);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Terminer(long h, int code);
        // 0 : terminé ; 258 : délai écoulé.
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Attendre(long h, int ms);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string Image(long h);
        // [handle, pid, entrée, sortie, erreurs] ou [-erreur].
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long[] Lancer(string ligne, string dossier, int redirections, bool sansFenetre);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Shell(string fichier, string parametres, string dossier, string verbe);
    }

    public enum ProcessWindowStyle { Normal = 0, Hidden = 1, Minimized = 2, Maximized = 3 }

    public enum ProcessPriorityClass
    {
        Normal = 0x20, Idle = 0x40, High = 0x80, RealTime = 0x100, BelowNormal = 0x4000, AboveNormal = 0x8000,
    }

    public sealed class ProcessStartInfo
    {
        private string _fichier = "", _arguments = "", _dossier = "", _verbe = "";

        public ProcessStartInfo() { UseShellExecute = true; }
        public ProcessStartInfo(string fileName) : this() { FileName = fileName; }
        public ProcessStartInfo(string fileName, string arguments) : this(fileName) { Arguments = arguments; }

        public string FileName { get { return _fichier; } set { _fichier = value ?? ""; } }
        public string Arguments { get { return _arguments; } set { _arguments = value ?? ""; } }
        public string WorkingDirectory { get { return _dossier; } set { _dossier = value ?? ""; } }
        public string Verb { get { return _verbe; } set { _verbe = value ?? ""; } }
        public bool UseShellExecute { get; set; }
        public bool CreateNoWindow { get; set; }
        public bool RedirectStandardInput { get; set; }
        public bool RedirectStandardOutput { get; set; }
        public bool RedirectStandardError { get; set; }
        public bool ErrorDialog { get; set; }
        public ProcessWindowStyle WindowStyle { get; set; }
        public Encoding StandardOutputEncoding { get; set; }
        public Encoding StandardErrorEncoding { get; set; }
        public string UserName { get; set; }
        public string Domain { get; set; }
        public bool LoadUserProfile { get; set; }
    }

    public class ProcessModule
    {
        private readonly string _chemin;
        internal ProcessModule(string chemin) { _chemin = chemin; }
        public string FileName { get { return _chemin; } }
        public string ModuleName { get { return Path.GetFileName(_chemin); } }
        public override string ToString() { return base.ToString() + " (" + ModuleName + ")"; }
    }

    public class Process : IDisposable
    {
        private const int StillActive = 259;

        private int _pid = -1;
        private long _h;
        private string _nom;
        private bool _fini;
        private int _code;
        private bool _signale;
        private ProcessStartInfo _infos;
        private StreamWriter _entree;
        private StreamReader _sortie, _erreurs;

        public Process() { }

        private Process(int pid, string nom)
        {
            _pid = pid;
            _nom = nom;
        }

        public event EventHandler Exited;

        public bool EnableRaisingEvents { get; set; }

        public ProcessStartInfo StartInfo
        {
            get { if (_infos == null) _infos = new ProcessStartInfo(); return _infos; }
            set { if (value == null) throw new ArgumentNullException("value"); _infos = value; }
        }

        // ── Recherche ──

        public static Process GetCurrentProcess()
        {
            int pid = ProcessusNatif.Courant();
            return new Process(pid, ProcessusNatif.Nom(pid));
        }

        // Le pid 0 est le processus « Idle » du système (#2071) : il existe
        // toujours, se nomme, mais ne s'ouvre pas (accès refusé).
        private const int Inactif = 0;

        public static Process GetProcessById(int processId)
        {
            if (processId == Inactif) return new Process(Inactif, "Idle");
            string nom = ProcessusNatif.Nom(processId);
            if (nom == null) throw new ArgumentException("Process with an Id of " + processId + " is not running.");
            return new Process(processId, nom);
        }

        public static Process GetProcessById(int processId, string machineName)
        {
            Locale(machineName);
            return GetProcessById(processId);
        }

        public static Process[] GetProcesses()
        {
            long[] liste = ProcessusNatif.Liste();
            List<Process> res = new List<Process>();
            res.Add(new Process(Inactif, "Idle"));
            for (int i = 0; i + 2 < liste.Length; i += 3)
            {
                int pid = (int)liste[i];
                string nom = ProcessusNatif.Nom(pid);
                if (pid != Inactif && nom != null) res.Add(new Process(pid, nom));
            }
            return res.ToArray();
        }

        public static Process[] GetProcesses(string machineName)
        {
            Locale(machineName);
            return GetProcesses();
        }

        public static Process[] GetProcessesByName(string processName)
        {
            if (processName == null) processName = "";
            List<Process> res = new List<Process>();
            foreach (Process p in GetProcesses())
            {
                if (string.Equals(p.ProcessName, processName, StringComparison.OrdinalIgnoreCase)) res.Add(p);
            }
            return res.ToArray();
        }

        public static Process[] GetProcessesByName(string processName, string machineName)
        {
            Locale(machineName);
            return GetProcessesByName(processName);
        }

        private static void Locale(string machine)
        {
            if (machine == null) throw new ArgumentNullException("machineName");
            if (machine != "." && machine.Length != 0 && !string.Equals(machine, Environment.MachineName, StringComparison.OrdinalIgnoreCase))
                throw new NotSupportedException("Feature is not supported for remote machines.");
        }

        // ── Propriétés ──

        private void Associe()
        {
            if (_pid < 0) throw new InvalidOperationException("No process is associated with this object.");
        }

        public int Id { get { Associe(); return _pid; } }

        public string ProcessName
        {
            get
            {
                Associe();
                if (_nom == null) _nom = _pid == Inactif ? "Idle" : ProcessusNatif.Nom(_pid);
                if (_nom == null) throw new InvalidOperationException("Process has exited, so the requested information is not available.");
                return _nom.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) ? _nom.Substring(0, _nom.Length - 4) : _nom;
            }
        }

        public string MachineName { get { Associe(); return "."; } }
        public bool Responding { get { return true; } }
        public IntPtr MainWindowHandle { get { return IntPtr.Zero; } }
        public string MainWindowTitle { get { return ""; } }
        public int SessionId { get { Associe(); return 1; } }
        public int BasePriority { get { return 8; } }

        // Handle du processus, ouvert à la demande. Le processus d'un autre
        // compte ne s'ouvre pas : Win32Exception « Access is denied. ».
        private long Poignee()
        {
            Associe();
            if (_pid == Inactif) throw new Win32Exception(5);
            if (_h > 0) return _h;
            long h = ProcessusNatif.Ouvrir(_pid);
            if (h == -87) throw new InvalidOperationException("Process has exited, so the requested information is not available.");
            if (h < 0) throw new Win32Exception((int)-h);
            _h = h;
            return h;
        }

        public IntPtr Handle { get { return new IntPtr(Poignee()); } }

        public ProcessModule MainModule
        {
            get
            {
                string chemin = ProcessusNatif.Image(Poignee());
                if (chemin == null) throw new Win32Exception(5);
                return new ProcessModule(chemin);
            }
        }

        public bool HasExited
        {
            get
            {
                if (_fini) return true;
                Associe();
                if (_pid == Inactif) throw new Win32Exception(5);
                long h = _h;
                if (h <= 0)
                {
                    h = ProcessusNatif.Ouvrir(_pid);
                    // Disparu (ou zombie, #1914) avant qu'on l'ouvre : fini.
                    if (h == -87) { Termine(-1); return true; }
                    if (h < 0) throw new Win32Exception((int)-h);
                    _h = h;
                }
                if (ProcessusNatif.Attendre(h, 0) != 0) return false;
                long code = ProcessusNatif.CodeSortie(h);
                Termine(code < 0 ? -1 : (int)code);
                return true;
            }
        }

        public int ExitCode
        {
            get
            {
                if (!HasExited) throw new InvalidOperationException("Process must exit before requested information can be determined.");
                return _code;
            }
        }

        private void Termine(int code)
        {
            if (_fini) return;
            _fini = true;
            _code = code;
            if (EnableRaisingEvents && !_signale)
            {
                _signale = true;
                EventHandler e = Exited;
                if (e != null) e(this, EventArgs.Empty);
            }
        }

        // ── Actions ──

        public void Kill()
        {
            long h = Poignee();
            if (HasExited) throw new InvalidOperationException("Cannot process request because the process has exited.");
            int r = ProcessusNatif.Terminer(h, -1);
            if (r < 0) throw new Win32Exception(-r);
        }

        public void WaitForExit() { WaitForExit(-1); }

        public bool WaitForExit(int milliseconds)
        {
            if (_fini) return true;
            long h = Poignee();
            if (ProcessusNatif.Attendre(h, milliseconds) != 0) return false;
            return HasExited;
        }

        public bool WaitForInputIdle() { return true; }
        public bool WaitForInputIdle(int milliseconds) { return true; }
        public void Refresh() { }
        public bool CloseMainWindow() { return false; }

        public StreamWriter StandardInput
        {
            get { if (_entree == null) throw new InvalidOperationException("StandardIn has not been redirected."); return _entree; }
        }

        public StreamReader StandardOutput
        {
            get { if (_sortie == null) throw new InvalidOperationException("StandardOut has not been redirected or the process hasn't started yet."); return _sortie; }
        }

        public StreamReader StandardError
        {
            get { if (_erreurs == null) throw new InvalidOperationException("StandardError has not been redirected."); return _erreurs; }
        }

        // ── Lancement ──

        public static Process Start(string fileName) { return Start(new ProcessStartInfo(fileName)); }

        public static Process Start(string fileName, string arguments) { return Start(new ProcessStartInfo(fileName, arguments)); }

        public static Process Start(ProcessStartInfo startInfo)
        {
            Process p = new Process();
            p.StartInfo = startInfo ?? throw new ArgumentNullException("startInfo");
            return p.Start() ? p : null;
        }

        public bool Start()
        {
            ProcessStartInfo s = StartInfo;
            if (s.FileName.Length == 0) throw new InvalidOperationException("Cannot start process because a file name has not been provided.");
            bool redirige = s.RedirectStandardInput || s.RedirectStandardOutput || s.RedirectStandardError;
            if (s.UseShellExecute)
            {
                if (redirige) throw new InvalidOperationException("The Process object must have the UseShellExecute property set to false in order to redirect IO streams.");
                int r = ProcessusNatif.Shell(s.FileName, s.Arguments.Length == 0 ? null : s.Arguments, s.WorkingDirectory.Length == 0 ? null : s.WorkingDirectory, s.Verb.Length == 0 ? null : s.Verb);
                if (r < 0) throw new Win32Exception(Shell(-r));
                // ShellExecute ne rend pas de processus.
                return false;
            }
            string fichier = s.FileName;
            string ligne = fichier.IndexOf(' ') >= 0 && fichier[0] != '"' ? "\"" + fichier + "\"" : fichier;
            if (s.Arguments.Length != 0) ligne += " " + s.Arguments;
            int quoi = (s.RedirectStandardInput ? 1 : 0) | (s.RedirectStandardOutput ? 2 : 0) | (s.RedirectStandardError ? 4 : 0);
            long[] res = ProcessusNatif.Lancer(ligne, s.WorkingDirectory.Length == 0 ? null : s.WorkingDirectory, quoi, s.CreateNoWindow);
            if (res.Length < 5) throw new Win32Exception((int)-res[0]);
            _h = res[0];
            _pid = (int)res[1];
            _nom = Path.GetFileName(fichier.Trim('"'));
            _fini = false;
            _signale = false;
            if (res[2] != 0) { _entree = new StreamWriter(new FileStream(res[2], FileAccess.Write), new UTF8Encoding(false)); _entree.AutoFlush = true; }
            if (res[3] != 0) _sortie = new StreamReader(new FileStream(res[3], FileAccess.Read), s.StandardOutputEncoding ?? Encoding.UTF8);
            if (res[4] != 0) _erreurs = new StreamReader(new FileStream(res[4], FileAccess.Read), s.StandardErrorEncoding ?? Encoding.UTF8);
            return true;
        }

        // Codes de ShellExecute (≤ 32) vers les erreurs Win32.
        private static int Shell(int code)
        {
            switch (code)
            {
                case 2: return 2;
                case 3: return 3;
                case 5: return 5;
                case 31: return 1155;
                default: return code == 0 ? 8 : code;
            }
        }

        // ── Fin ──

        public void Close()
        {
            if (_h > 0) { System.IO.Natif.Fermer(_h); _h = 0; }
            if (_entree != null) { try { _entree.Dispose(); } catch (IOException) { } _entree = null; }
            if (_sortie != null) { _sortie.Dispose(); _sortie = null; }
            if (_erreurs != null) { _erreurs.Dispose(); _erreurs = null; }
        }

        public void Dispose() { Close(); }

        public override string ToString()
        {
            if (_pid < 0) return base.ToString();
            string nom = _nom == null ? null : ProcessName;
            return base.ToString() + " (" + nom + ")";
        }
    }
}
