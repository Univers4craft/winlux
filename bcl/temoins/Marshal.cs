// Témoin de Marshal.SizeOf dans la mscorlib Winlux (#2026) : la taille
// marshalée, pas la taille gérée ni 8 en dur.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 6) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait Marshal::SizeOf pour remplir le
// champ cbSize de structures Win32 avant leurs appels P/Invoke.
#pragma warning disable 169, 649
using System;
using System.Runtime.InteropServices;

namespace Temoins
{
    struct Deux { public int A; public long B; }                                  // 16
    struct Booleen { public bool A; public byte B; }                              // 8 : BOOL Win32
    [StructLayout(LayoutKind.Sequential, Pack = 1)] struct Serre { public byte A; public int B; }   // 5
    [StructLayout(LayoutKind.Sequential, Size = 32)] struct Taille { public int A; }               // 32
    [StructLayout(LayoutKind.Explicit)] struct Union { [FieldOffset(0)] public int A; [FieldOffset(0)] public long B; [FieldOffset(8)] public short C; } // 16
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct Unicode { public char C; [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 10)] public string Nom; public int N; } // 2+20=22, int à 24 : 28
    struct Ansi { public char C; [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 10)] public string Nom; } // 11
    struct Tableau { [MarshalAs(UnmanagedType.ByValArray, SizeConst = 4)] public short[] V; public IntPtr P; } // 8 + 8
    struct Imbrique { public byte X; public Deux D; }                             // 24
    // SP_DEVICE_INTERFACE_DATA : cbSize, Guid, Flags, Reserved (32 en 64 bits).
    struct InterfacePeripherique { public int cbSize; public Guid InterfaceClassGuid; public int Flags; public IntPtr Reserved; }
    struct AvecChaine { public int A; public string S; }                          // 16 : pointeur
    class SansDisposition { public int A; }
    [StructLayout(LayoutKind.Sequential)] class AvecDisposition { public int A; public short B; } // 8

    static class Marshaling
    {
        static int Main()
        {
            if (Marshal.SizeOf(typeof(int)) != 4 || Marshal.SizeOf(typeof(bool)) != 4 || Marshal.SizeOf(typeof(char)) != 1 || Marshal.SizeOf(typeof(IntPtr)) != 8 || Marshal.SizeOf(typeof(double)) != 8) return 1;
            if (Marshal.SizeOf(typeof(Deux)) != 16 || Marshal.SizeOf(typeof(Booleen)) != 8 || Marshal.SizeOf(typeof(Serre)) != 5 || Marshal.SizeOf(typeof(Taille)) != 32) return 2;
            if (Marshal.SizeOf(typeof(Union)) != 16 || Marshal.SizeOf(typeof(Unicode)) != 28 || Marshal.SizeOf(typeof(Ansi)) != 11 || Marshal.SizeOf(typeof(Tableau)) != 16) return 3;
            if (Marshal.SizeOf(typeof(Imbrique)) != 24 || Marshal.SizeOf(typeof(InterfacePeripherique)) != 32 || Marshal.SizeOf(typeof(AvecChaine)) != 16) return 4;
            // Formes génériques et sur une instance.
            if (Marshal.SizeOf<InterfacePeripherique>() != 32 || Marshal.SizeOf(new Deux()) != 16 || Marshal.SizeOf((object)new Serre()) != 5 || Marshal.SizeOf(new AvecDisposition()) != 8) return 5;
            try { Marshal.SizeOf(typeof(SansDisposition)); return 6; } catch (ArgumentException) { }
            try { Marshal.SizeOf((Type)null); return 6; } catch (ArgumentNullException) { }
            return 200;
        }
    }
}
