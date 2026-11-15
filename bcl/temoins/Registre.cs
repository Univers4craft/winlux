// Témoin de Microsoft.Win32.Registry (#2020), de SafeHandle et du passage
// d'un SafeHandle aux appels P/Invoke (#2019) et des membres d'IntPtr
// (#2018) dans la mscorlib Winlux.
// Code de sortie attendu : 200.
// Option de compilation : -unsafe.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// La 9 vérifie que « is T[] » regarde le type des éléments (#2023).
// Corsair.Service.exe (iCUE 4.18) appelait RegistryKey::OpenSubKey (7 fois),
// GetValue (5), OpenBaseKey (2), IntPtr::op_Explicit (10),
// SafeHandle::DangerousGetHandle/IsInvalid/IsClosed (3) et
// SafeWaitHandle::.ctor (2).
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using Microsoft.Win32;
using Microsoft.Win32.SafeHandles;

namespace Temoins
{
    enum Couleur { Rouge }

    static class Registre
    {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern SafeFileHandle CreateFileW(string name, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);

        [DllImport("kernel32.dll")]
        static extern uint GetFileSize(SafeFileHandle file, IntPtr high);

        static unsafe int Main()
        {
            // IntPtr : conversions, opérateurs, pointeurs.
            IntPtr a = new IntPtr(0x1234);
            if ((long)a != 0x1234 || a.ToInt32() != 0x1234 || (a + 4).ToInt64() != 0x1238 || a == IntPtr.Zero || IntPtr.Size != 8) return 1;
            int x = 41;
            IntPtr p = (IntPtr)(&x);
            *(int*)p.ToPointer() += 1;
            if (x != 42 || !new IntPtr(5).Equals((object)(IntPtr)5L) || new UIntPtr(7u).ToUInt64() != 7) return 1;

            // Registre : écrire, relire, énumérer, supprimer, sous HKCU.
            string chemin = @"Software\Winlux\Temoin-" + Environment.TickCount;
            using (RegistryKey cle = Registry.CurrentUser.CreateSubKey(chemin))
            {
                if (cle == null || cle.Name != @"HKEY_CURRENT_USER\" + chemin) return 2;
                cle.SetValue("Texte", "été");
                cle.SetValue("Mot", 42);
                cle.SetValue("Long", 1L << 40, RegistryValueKind.QWord);
                cle.SetValue("Lignes", new string[] { "un", "deux" });
                cle.SetValue("Octets", new byte[] { 1, 2, 3 });
                cle.SetValue("Chemin", "%WINLUX_TEMOIN_INCONNU%\\x", RegistryValueKind.ExpandString);
                cle.CreateSubKey("Enfant").Close();
            }
            using (RegistryKey cle = Registry.CurrentUser.OpenSubKey(chemin))
            {
                if (cle == null) return 3;
                if ((string)cle.GetValue("Texte") != "été" || (int)cle.GetValue("Mot") != 42 || (long)cle.GetValue("Long") != 1L << 40) return 3;
                string[] lignes = (string[])cle.GetValue("Lignes");
                byte[] octets = (byte[])cle.GetValue("Octets");
                if (lignes.Length != 2 || lignes[1] != "deux" || octets.Length != 3 || octets[2] != 3) return 3;
                if ((string)cle.GetValue("Chemin") != "%WINLUX_TEMOIN_INCONNU%\\x" || cle.GetValue("Absente", "défaut") as string != "défaut") return 3;
                if (cle.GetValueKind("Mot") != RegistryValueKind.DWord || cle.GetValueKind("Lignes") != RegistryValueKind.MultiString || cle.GetValueKind("Chemin") != RegistryValueKind.ExpandString) return 4;
                if (cle.ValueCount != 6 || cle.SubKeyCount != 1 || cle.GetSubKeyNames()[0] != "Enfant") return 4;
                // Clé ouverte en lecture seule : écrire est refusé.
                try { cle.SetValue("Interdit", 1); return 4; } catch (UnauthorizedAccessException) { }
            }
            if (Registry.GetValue(@"HKEY_CURRENT_USER\" + chemin, "Mot", null) is int == false) return 5;
            using (RegistryKey cle = Registry.CurrentUser.OpenSubKey(chemin, true))
            {
                cle.DeleteValue("Mot");
                try { cle.DeleteValue("Mot"); return 5; } catch (ArgumentException) { }
                cle.DeleteValue("Mot", false);
                if (cle.GetValue("Mot") != null) return 5;
            }
            try { Registry.CurrentUser.DeleteSubKey(chemin); return 5; } catch (InvalidOperationException) { }
            Registry.CurrentUser.DeleteSubKeyTree(chemin);
            if (Registry.CurrentUser.OpenSubKey(chemin) != null) return 5;

            // Vues 32 et 64 bits de HKLM\Software : toutes deux ouvrables.
            using (RegistryKey hklm64 = RegistryKey.OpenBaseKey(RegistryHive.LocalMachine, RegistryView.Registry64))
            using (RegistryKey hklm32 = RegistryKey.OpenBaseKey(RegistryHive.LocalMachine, RegistryView.Registry32))
            {
                if (hklm64.View != RegistryView.Registry64 || hklm64.OpenSubKey("Software") == null || hklm32.OpenSubKey("Software") == null) return 6;
            }

            // SafeFileHandle rendu par P/Invoke puis passé à un autre appel.
            string fichier = Path.Combine(Path.GetTempPath(), "temoin-registre-" + Environment.TickCount);
            File.WriteAllBytes(fichier, new byte[123]);
            SafeFileHandle h = CreateFileW(fichier, 0x80000000, 1, IntPtr.Zero, 3, 0x80, IntPtr.Zero);
            if (h == null || h.IsInvalid || h.IsClosed) return 7;
            if (GetFileSize(h, IntPtr.Zero) != 123) return 7;
            h.Dispose();
            if (!h.IsClosed) return 7;
            File.Delete(fichier);
            SafeFileHandle invalide = CreateFileW(fichier + ".absent", 0x80000000, 1, IntPtr.Zero, 3, 0x80, IntPtr.Zero);
            if (!invalide.IsInvalid) return 7;

            // SafeWaitHandle d'un objet du noyau.
            using (Mutex m = new Mutex())
            {
                SafeWaitHandle sw = m.SafeWaitHandle;
                if (sw.IsInvalid || sw.DangerousGetHandle() == IntPtr.Zero) return 8;
            }
            if (!new ManualResetEvent(false).SafeWaitHandle.IsInvalid) return 8;

            object octets9 = new byte[] { 1 }, chaines9 = new string[] { "x" }, entiers9 = new int[] { 1 }, couleurs9 = new Couleur[] { Couleur.Rouge };
            if (octets9 is string[] || chaines9 is byte[] || entiers9 is long[] || octets9 is int[]) return 9;
            if (!(chaines9 is object[]) || !(entiers9 is uint[]) || !(couleurs9 is int[]) || !(octets9 is Array)) return 9;
            try { string[] faux = (string[])octets9; return 9; } catch (InvalidCastException) { }
            return 200;
        }
    }
}
