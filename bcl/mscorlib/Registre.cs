// Microsoft.Win32.Registry et RegistryKey de la mscorlib Winlux (#2020).
// Licence GPL 3. Écrit d'après la documentation publique de .NET Framework
// 4.x (MS Learn : « RegistryKey Class », « RegistryValueKind »,
// « RegistryView ») ; aucun code de Microsoft, de Mono ou de Wine.
//
// Les clés passent par les primitives de RegistreNatif (clr/registre.rs),
// qui appellent advapi32 : un programme .NET lit le même registre, dans la
// même vue 32/64 bits, qu'un programme natif.
using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.CompilerServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace Microsoft.Win32
{
    public enum RegistryHive
    {
        ClassesRoot = unchecked((int)0x80000000), CurrentUser = unchecked((int)0x80000001),
        LocalMachine = unchecked((int)0x80000002), Users = unchecked((int)0x80000003),
        PerformanceData = unchecked((int)0x80000004), CurrentConfig = unchecked((int)0x80000005),
        DynData = unchecked((int)0x80000006),
    }

    public enum RegistryView { Default = 0, Registry64 = 0x100, Registry32 = 0x200 }

    public enum RegistryValueKind
    {
        Unknown = 0, String = 1, ExpandString = 2, Binary = 3, DWord = 4, MultiString = 7, QWord = 11, None = -1,
    }

    [Flags]
    public enum RegistryValueOptions { None = 0, DoNotExpandEnvironmentNames = 1 }

    public enum RegistryKeyPermissionCheck { Default = 0, ReadSubTree = 1, ReadWriteSubTree = 2 }

    [Flags]
    public enum RegistryOptions { None = 0, Volatile = 1 }

    // Primitives du moteur (registre.rs). Un échec rend -code Win32.
    internal static class RegistreNatif
    {
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long Ouvrir(long racine, string sousCle, int acces);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern long Creer(long racine, string sousCle, int acces);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Fermer(long h);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Vider(long h);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Type(long h, string nom);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern byte[] Lire(long h, string nom);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int Ecrire(long h, string nom, int type, byte[] donnees);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int SupprimerValeur(long h, string nom);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int SupprimerCle(long h, string sousCle, int vue);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern int SupprimerArbre(long h, string sousCle);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string[] SousCles(long h);
        [MethodImpl(MethodImplOptions.InternalCall)]
        internal static extern string[] Valeurs(long h);
    }

    public static class Registry
    {
        public static readonly RegistryKey ClassesRoot = RegistryKey.Racine(RegistryHive.ClassesRoot, RegistryView.Default);
        public static readonly RegistryKey CurrentUser = RegistryKey.Racine(RegistryHive.CurrentUser, RegistryView.Default);
        public static readonly RegistryKey LocalMachine = RegistryKey.Racine(RegistryHive.LocalMachine, RegistryView.Default);
        public static readonly RegistryKey Users = RegistryKey.Racine(RegistryHive.Users, RegistryView.Default);
        public static readonly RegistryKey PerformanceData = RegistryKey.Racine(RegistryHive.PerformanceData, RegistryView.Default);
        public static readonly RegistryKey CurrentConfig = RegistryKey.Racine(RegistryHive.CurrentConfig, RegistryView.Default);

        // « HKEY_CURRENT_USER\Software\X » : la racine et le reste.
        private static RegistryKey Separer(string keyName, out string reste)
        {
            if (keyName == null) throw new ArgumentNullException("keyName");
            int i = keyName.IndexOf('\\');
            string racine = (i < 0 ? keyName : keyName.Substring(0, i)).ToUpperInvariant();
            reste = i < 0 ? "" : keyName.Substring(i + 1);
            switch (racine)
            {
                case "HKEY_CURRENT_USER": return CurrentUser;
                case "HKEY_LOCAL_MACHINE": return LocalMachine;
                case "HKEY_CLASSES_ROOT": return ClassesRoot;
                case "HKEY_USERS": return Users;
                case "HKEY_PERFORMANCE_DATA": return PerformanceData;
                case "HKEY_CURRENT_CONFIG": return CurrentConfig;
                default: throw new ArgumentException("Registry key name must start with a valid base key name.", "keyName");
            }
        }

        public static object GetValue(string keyName, string valueName, object defaultValue)
        {
            string reste;
            RegistryKey racine = Separer(keyName, out reste);
            using (RegistryKey cle = racine.OpenSubKey(reste))
                return cle == null ? null : cle.GetValue(valueName, defaultValue);
        }

        public static void SetValue(string keyName, string valueName, object value) { SetValue(keyName, valueName, value, RegistryValueKind.Unknown); }

        public static void SetValue(string keyName, string valueName, object value, RegistryValueKind valueKind)
        {
            string reste;
            RegistryKey racine = Separer(keyName, out reste);
            using (RegistryKey cle = racine.CreateSubKey(reste)) cle.SetValue(valueName, value, valueKind);
        }
    }

    public sealed class RegistryKey : IDisposable
    {
        private const int KeyRead = 0x20019;
        private const int KeyWrite = 0x20006;

        private long _h;
        private readonly string _nom;
        private readonly bool _ecriture;
        private readonly bool _racine;
        private readonly RegistryView _vue;

        private RegistryKey(long h, string nom, bool ecriture, bool racine, RegistryView vue)
        {
            _h = h;
            _nom = nom;
            _ecriture = ecriture;
            _racine = racine;
            _vue = vue;
        }

        internal static RegistryKey Racine(RegistryHive hive, RegistryView vue)
        {
            string nom;
            switch (hive)
            {
                case RegistryHive.ClassesRoot: nom = "HKEY_CLASSES_ROOT"; break;
                case RegistryHive.CurrentUser: nom = "HKEY_CURRENT_USER"; break;
                case RegistryHive.LocalMachine: nom = "HKEY_LOCAL_MACHINE"; break;
                case RegistryHive.Users: nom = "HKEY_USERS"; break;
                case RegistryHive.PerformanceData: nom = "HKEY_PERFORMANCE_DATA"; break;
                case RegistryHive.CurrentConfig: nom = "HKEY_CURRENT_CONFIG"; break;
                case RegistryHive.DynData: nom = "HKEY_DYN_DATA"; break;
                default: throw new ArgumentException("Registry HKEY was out of the legal range.", "hKey");
            }
            return new RegistryKey((long)(int)hive, nom, true, true, vue);
        }

        public static RegistryKey OpenBaseKey(RegistryHive hKey, RegistryView view)
        {
            if (view != RegistryView.Default && view != RegistryView.Registry32 && view != RegistryView.Registry64)
                throw new ArgumentException("The specified RegistryView value is not valid.", "view");
            return Racine(hKey, view);
        }

        public static RegistryKey FromHandle(SafeRegistryHandle handle) { return FromHandle(handle, RegistryView.Default); }

        public static RegistryKey FromHandle(SafeRegistryHandle handle, RegistryView view)
        {
            if (handle == null) throw new ArgumentNullException("handle");
            return new RegistryKey(handle.DangerousGetHandle().ToInt64(), "", true, true, view);
        }

        public string Name { get { Ouvert(); return _nom; } }
        public RegistryView View { get { Ouvert(); return _vue; } }

        public SafeRegistryHandle Handle
        {
            get { Ouvert(); return new SafeRegistryHandle(new IntPtr(_h), false); }
        }

        public int SubKeyCount { get { return GetSubKeyNames().Length; } }
        public int ValueCount { get { return GetValueNames().Length; } }

        private void Ouvert()
        {
            if (_h == 0) throw new ObjectDisposedException("RegistryKey", "Cannot access a closed registry key.");
        }

        private void Ecrivable()
        {
            Ouvert();
            if (!_ecriture) throw new UnauthorizedAccessException("Cannot write to the registry key.");
        }

        private static Exception Erreur(int code, string quoi)
        {
            if (code < 0) code = -code;
            switch (code)
            {
                case 5: return new UnauthorizedAccessException("Access to the registry key '" + quoi + "' is denied.");
                case 2: return new IOException("The specified registry key does not exist.");
                default: return new IOException("Registry error " + code + " on '" + quoi + "'.");
            }
        }

        private static string Normaliser(string name)
        {
            if (name == null) throw new ArgumentNullException("name");
            while (name.EndsWith("\\")) name = name.Substring(0, name.Length - 1);
            if (name.Length > 255) throw new ArgumentException("Registry key name must be less than 255 characters.", "name");
            return name;
        }

        private int Acces(bool ecriture) { return (ecriture ? KeyRead | KeyWrite : KeyRead) | (int)_vue; }

        public RegistryKey OpenSubKey(string name) { return OpenSubKey(name, false); }

        public RegistryKey OpenSubKey(string name, RegistryKeyPermissionCheck permissionCheck)
        {
            return OpenSubKey(name, permissionCheck == RegistryKeyPermissionCheck.ReadWriteSubTree);
        }

        // Clé absente : null, comme .NET.
        public RegistryKey OpenSubKey(string name, bool writable)
        {
            name = Normaliser(name);
            Ouvert();
            long h = RegistreNatif.Ouvrir(_h, name, Acces(writable));
            if (h == -2 || h == -3) return null;
            if (h < 0) throw Erreur((int)h, _nom + "\\" + name);
            return new RegistryKey(h, name.Length == 0 ? _nom : _nom + "\\" + name, writable, false, _vue);
        }

        public RegistryKey CreateSubKey(string subkey) { return CreateSubKey(subkey, _ecriture); }

        public RegistryKey CreateSubKey(string subkey, RegistryKeyPermissionCheck permissionCheck)
        {
            return CreateSubKey(subkey, permissionCheck != RegistryKeyPermissionCheck.ReadSubTree);
        }

        public RegistryKey CreateSubKey(string subkey, RegistryKeyPermissionCheck permissionCheck, RegistryOptions registryOptions)
        {
            return CreateSubKey(subkey, permissionCheck);
        }

        public RegistryKey CreateSubKey(string subkey, bool writable)
        {
            subkey = Normaliser(subkey);
            Ecrivable();
            long h = RegistreNatif.Creer(_h, subkey, Acces(writable));
            if (h < 0) throw Erreur((int)h, _nom + "\\" + subkey);
            return new RegistryKey(h, subkey.Length == 0 ? _nom : _nom + "\\" + subkey, writable, false, _vue);
        }

        public void DeleteSubKey(string subkey) { DeleteSubKey(subkey, true); }

        public void DeleteSubKey(string subkey, bool throwOnMissingSubKey)
        {
            subkey = Normaliser(subkey);
            Ecrivable();
            using (RegistryKey cle = OpenSubKey(subkey))
            {
                if (cle == null)
                {
                    if (throwOnMissingSubKey) throw new ArgumentException("Cannot delete a subkey tree because the subkey does not exist.");
                    return;
                }
                if (cle.SubKeyCount > 0) throw new InvalidOperationException("Registry key has subkeys and recursive removes are not supported by this method.");
            }
            int r = RegistreNatif.SupprimerCle(_h, subkey, (int)_vue);
            if (r < 0 && !(r == -2 && !throwOnMissingSubKey)) throw Erreur(r, _nom + "\\" + subkey);
        }

        public void DeleteSubKeyTree(string subkey) { DeleteSubKeyTree(subkey, true); }

        public void DeleteSubKeyTree(string subkey, bool throwOnMissingSubKey)
        {
            subkey = Normaliser(subkey);
            if (subkey.Length == 0) throw new ArgumentException("Cannot delete a registry hive's subtree.");
            Ecrivable();
            int r = RegistreNatif.SupprimerArbre(_h, subkey);
            if (r == -2)
            {
                if (throwOnMissingSubKey) throw new ArgumentException("Cannot delete a subkey tree because the subkey does not exist.");
                return;
            }
            if (r < 0) throw Erreur(r, _nom + "\\" + subkey);
        }

        public void DeleteValue(string name) { DeleteValue(name, true); }

        public void DeleteValue(string name, bool throwOnMissingValue)
        {
            Ecrivable();
            int r = RegistreNatif.SupprimerValeur(_h, name);
            if (r == -2)
            {
                if (throwOnMissingValue) throw new ArgumentException("No value exists with that name.");
                return;
            }
            if (r < 0) throw Erreur(r, _nom);
        }

        public string[] GetSubKeyNames()
        {
            Ouvert();
            return RegistreNatif.SousCles(_h) ?? new string[0];
        }

        public string[] GetValueNames()
        {
            Ouvert();
            return RegistreNatif.Valeurs(_h) ?? new string[0];
        }

        public RegistryValueKind GetValueKind(string name)
        {
            Ouvert();
            int t = RegistreNatif.Type(_h, name);
            if (t < 0) throw new IOException("The specified registry key does not exist.");
            switch (t)
            {
                case 0: return RegistryValueKind.None;
                case 1: case 2: case 3: case 4: case 7: case 11: return (RegistryValueKind)t;
                default: return RegistryValueKind.Unknown;
            }
        }

        public object GetValue(string name) { return GetValue(name, null, RegistryValueOptions.None); }
        public object GetValue(string name, object defaultValue) { return GetValue(name, defaultValue, RegistryValueOptions.None); }

        public object GetValue(string name, object defaultValue, RegistryValueOptions options)
        {
            Ouvert();
            int type = RegistreNatif.Type(_h, name);
            if (type < 0) return defaultValue;
            byte[] d = RegistreNatif.Lire(_h, name);
            if (d == null) return defaultValue;
            switch (type)
            {
                case 1:
                    return Chaine(d, 0, d.Length);
                case 2:
                    string s = Chaine(d, 0, d.Length);
                    return (options & RegistryValueOptions.DoNotExpandEnvironmentNames) != 0 ? s : Environment.ExpandEnvironmentVariables(s);
                case 4:
                    if (d.Length < 4) return defaultValue;
                    return BitConverter.ToInt32(d, 0);
                case 11:
                    if (d.Length < 8) return defaultValue;
                    return BitConverter.ToInt64(d, 0);
                case 7:
                    return Multiple(d);
                default:
                    return d;
            }
        }

        // UTF-16LE, sans les zéros de fin.
        private static string Chaine(byte[] d, int debut, int fin)
        {
            int n = (fin - debut) / 2;
            char[] c = new char[n];
            for (int i = 0; i < n; i++) c[i] = (char)(d[debut + 2 * i] | d[debut + 2 * i + 1] << 8);
            int l = n;
            while (l > 0 && c[l - 1] == '\0') l--;
            return new string(c, 0, l);
        }

        private static string[] Multiple(byte[] d)
        {
            List<string> r = new List<string>();
            int debut = 0;
            for (int i = 0; i + 1 < d.Length; i += 2)
            {
                if (d[i] == 0 && d[i + 1] == 0)
                {
                    if (i == debut) break;
                    r.Add(Chaine(d, debut, i));
                    debut = i + 2;
                }
            }
            if (debut + 1 < d.Length && debut < d.Length - 1)
            {
                string reste = Chaine(d, debut, d.Length);
                if (reste.Length > 0) r.Add(reste);
            }
            return r.ToArray();
        }

        private static byte[] Utf16(string s, bool zero)
        {
            byte[] b = new byte[(s.Length + (zero ? 1 : 0)) * 2];
            for (int i = 0; i < s.Length; i++) { b[2 * i] = (byte)s[i]; b[2 * i + 1] = (byte)(s[i] >> 8); }
            return b;
        }

        public void SetValue(string name, object value) { SetValue(name, value, RegistryValueKind.Unknown); }

        public void SetValue(string name, object value, RegistryValueKind valueKind)
        {
            if (value == null) throw new ArgumentNullException("value");
            if (name != null && name.Length > 16383) throw new ArgumentException("Registry value names should not be greater than 16,383 characters.", "name");
            Ecrivable();
            if (valueKind == RegistryValueKind.Unknown)
            {
                if (value is int) valueKind = RegistryValueKind.DWord;
                else if (value is string[]) valueKind = RegistryValueKind.MultiString;
                else if (value is byte[]) valueKind = RegistryValueKind.Binary;
                else valueKind = RegistryValueKind.String;
            }
            byte[] d;
            int type = (int)valueKind;
            try
            {
                switch (valueKind)
                {
                    case RegistryValueKind.String:
                    case RegistryValueKind.ExpandString:
                        d = Utf16(value.ToString(), true);
                        break;
                    case RegistryValueKind.DWord:
                        d = BitConverter.GetBytes(Convert.ToInt32(value));
                        break;
                    case RegistryValueKind.QWord:
                        d = BitConverter.GetBytes(Convert.ToInt64(value));
                        break;
                    case RegistryValueKind.MultiString:
                        string[] lignes = (string[])value;
                        StringBuilder sb = new StringBuilder();
                        foreach (string l in lignes)
                        {
                            if (l == null) throw new ArgumentException("RegistryKey.SetValue does not allow a String[] that contains a null String reference.");
                            sb.Append(l).Append('\0');
                        }
                        d = Utf16(sb.ToString(), true);
                        break;
                    case RegistryValueKind.Binary:
                    case RegistryValueKind.None:
                        d = (byte[])value;
                        type = valueKind == RegistryValueKind.None ? 0 : 3;
                        break;
                    default:
                        throw new ArgumentException("The specified RegistryValueKind is an invalid value.", "valueKind");
                }
            }
            catch (InvalidCastException)
            {
                throw new ArgumentException("The type of the value object did not match the specified RegistryValueKind or the object could not be properly converted.");
            }
            catch (FormatException)
            {
                throw new ArgumentException("The type of the value object did not match the specified RegistryValueKind or the object could not be properly converted.");
            }
            int r = RegistreNatif.Ecrire(_h, name, type, d);
            if (r < 0) throw Erreur(r, _nom);
        }

        public void Flush() { if (_h != 0 && !_racine) RegistreNatif.Vider(_h); }

        public void Close() { Dispose(); }

        public void Dispose()
        {
            long h = _h;
            if (h == 0 || _racine) return;
            _h = 0;
            RegistreNatif.Fermer(h);
        }

        public override string ToString() { Ouvert(); return _nom; }
    }
}
