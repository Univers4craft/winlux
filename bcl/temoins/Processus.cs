// Témoin de System.Diagnostics.Process dans la mscorlib Winlux (#2050).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// La 9 vérifie le processus « Idle » de pid 0, comme sous Windows (#2071).
// Le témoin se relance lui-même comme processus enfant (« enfant N » rend N
// après avoir écrit sur sa sortie, « attente » dort 30 s).
// Corsair.Service.exe (iCUE 4.18) appelait Process::GetProcessById, puis
// GetCurrentProcess, GetProcessesByName, HasExited et Kill.
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Threading.Tasks;

namespace Temoins
{
    static class Processus
    {
        static int Main(string[] args)
        {
            if (args.Length == 2 && args[0] == "enfant")
            {
                Console.WriteLine("bonjour du fils");
                return int.Parse(args[1]);
            }
            if (args.Length == 1 && args[0] == "attente")
            {
                Task.Delay(30000).Wait();
                return 1;
            }

            // Le processus courant.
            Process moi = Process.GetCurrentProcess();
            if (moi.Id <= 0 || moi.ProcessName.Length == 0 || moi.ProcessName.EndsWith(".exe") || moi.HasExited) return 1;
            Process lui = Process.GetProcessById(moi.Id);
            if (lui.Id != moi.Id || lui.ProcessName != moi.ProcessName) return 2;

            // Un pid qui n'existe pas : ArgumentException.
            try { Process.GetProcessById(0x7FFFFFF0); return 2; } catch (ArgumentException) { }

            bool trouve = false;
            foreach (Process p in Process.GetProcesses()) if (p.Id == moi.Id) trouve = true;
            if (!trouve) return 3;
            trouve = false;
            foreach (Process p in Process.GetProcessesByName(moi.ProcessName.ToUpperInvariant())) if (p.Id == moi.Id) trouve = true;
            if (!trouve || Process.GetProcessesByName("winlux-aucun-processus").Length != 0) return 3;

            string exe = moi.MainModule.FileName;
            if (!exe.EndsWith(".exe", StringComparison.OrdinalIgnoreCase)) return 4;

            // Enfant redirigé : sortie, erreurs (vides), code de sortie.
            ProcessStartInfo si = new ProcessStartInfo(exe, "enfant 7");
            si.UseShellExecute = false;
            si.RedirectStandardOutput = true;
            si.RedirectStandardError = true;
            si.CreateNoWindow = true;
            using (Process fils = Process.Start(si))
            {
                if (fils == null || fils.Id <= 0) return 5;
                string sortie = fils.StandardOutput.ReadToEnd();
                string erreurs = fils.StandardError.ReadToEnd();
                if (!fils.WaitForExit(20000)) return 5;
                if (sortie.Trim() != "bonjour du fils" || erreurs.Length != 0) return 5;
                if (!fils.HasExited || fils.ExitCode != 7) return 5;
                try { fils.Kill(); return 5; } catch (InvalidOperationException) { }
            }

            // Kill d'un enfant qui dort.
            ProcessStartInfo dort = new ProcessStartInfo(exe, "attente");
            dort.UseShellExecute = false;
            dort.CreateNoWindow = true;
            Process dormeur = Process.Start(dort);
            if (dormeur.HasExited || dormeur.WaitForExit(200)) return 6;
            bool fini = false;
            dormeur.EnableRaisingEvents = true;
            dormeur.Exited += (o, e) => fini = true;
            dormeur.Kill();
            if (!dormeur.WaitForExit(10000) || !dormeur.HasExited || !fini) return 6;

            // Un enfant fini que personne n'a encore attendu reste un zombie
            // côté Linux : pour Windows, il est terminé (#1914).
            ProcessStartInfo vite = new ProcessStartInfo(exe, "enfant 3");
            vite.UseShellExecute = false;
            vite.RedirectStandardOutput = true;
            vite.RedirectStandardError = true;
            vite.CreateNoWindow = true;
            Process bref = Process.Start(vite);
            bref.StandardOutput.ReadToEnd();
            bref.StandardError.ReadToEnd();
            Task.Delay(1500).Wait();
            try { Process.GetProcessById(bref.Id); return 7; } catch (ArgumentException) { }
            foreach (Process p in Process.GetProcesses()) if (p.Id == bref.Id) return 7;
            if (!bref.HasExited || bref.ExitCode != 3) return 7;

            // Fichier absent : Win32Exception 2.
            ProcessStartInfo absent = new ProcessStartInfo(System.IO.Path.Combine(System.IO.Path.GetTempPath(), "winlux-absent-2050.exe"));
            absent.UseShellExecute = false;
            try { Process.Start(absent); return 8; }
            catch (Win32Exception e) { if (e.NativeErrorCode != 2) return 8; }
            try { new Process().Start(); return 8; } catch (InvalidOperationException) { }

            // Pid 0 : « Idle », listé, nommé, mais jamais ouvert (#2071).
            Process idle = Process.GetProcessById(0);
            if (idle.Id != 0 || idle.ProcessName != "Idle") return 9;
            trouve = false;
            foreach (Process p in Process.GetProcesses()) if (p.Id == 0 && p.ProcessName == "Idle") trouve = true;
            if (!trouve) return 9;
            try { idle.Kill(); return 9; } catch (Win32Exception) { }
            try { object m = idle.MainModule; return 9; } catch (Win32Exception) { }
            try { bool b = idle.HasExited; return 9; } catch (Win32Exception) { }
            return 200;
        }
    }
}
