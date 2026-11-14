// Témoin de Marshal dans la mscorlib Winlux (#2105) : mémoire native,
// copies, chaînes natives, structures et HRESULT.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 8) de la première vérification qui échoue.
// L'interface d'iCUE 4.18 levait six exceptions non gérées sur
// Marshal::StructureToPtr.
#pragma warning disable 169, 649
using System;
using System.Runtime.InteropServices;

namespace Temoins
{
    struct Point { public int X; public int Y; }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct Fiche
    {
        public int Id;                                                            // 0
        public bool Actif;                                                        // 4 : BOOL
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 8)] public string Code;   // 8 : 16 octets
        public string Nom;                                                        // 24 : LPWSTR
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 3)] public short[] Valeurs; // 32 : 6 octets
        public Point P;                                                           // 40
        public double D;                                                          // 48
        [MarshalAs(UnmanagedType.U1)] public bool Court;                          // 56
        public char C;                                                            // 58 ; taille 64
    }

    struct FicheAnsi { public char C; [MarshalAs(UnmanagedType.LPStr)] public string S; public byte B; } // 0, 8, 16 ; 24

    [StructLayout(LayoutKind.Sequential)]
    class Boite { public int A; public string S; public Boite() { A = -1; } }

    static class Interop
    {
        static int Main()
        {
            // 1. Mémoire native, lectures et écritures brutes.
            IntPtr m = Marshal.AllocHGlobal(32);
            if (m == IntPtr.Zero) return 1;
            Marshal.WriteInt32(m, 0x11223344);
            Marshal.WriteInt64(m, 8, 0x0102030405060708L);
            Marshal.WriteByte(m, 4, 0xAB);
            Marshal.WriteInt16(m, 6, 'Z');
            Marshal.WriteIntPtr(m, 16, new IntPtr(0x7FFF0000));
            if (Marshal.ReadInt32(m) != 0x11223344 || Marshal.ReadByte(m, 0) != 0x44 || Marshal.ReadByte(m, 4) != 0xAB) return 1;
            if (Marshal.ReadInt16(m, 6) != 'Z' || Marshal.ReadInt64(m, 8) != 0x0102030405060708L || Marshal.ReadIntPtr(m, 16) != new IntPtr(0x7FFF0000)) return 1;
            m = Marshal.ReAllocHGlobal(m, new IntPtr(4096));
            if (Marshal.ReadInt64(m, 8) != 0x0102030405060708L) return 1;
            Marshal.FreeHGlobal(m);
            IntPtr co = Marshal.AllocCoTaskMem(8);
            Marshal.WriteInt64(co, -1L);
            co = Marshal.ReAllocCoTaskMem(co, 64);
            if (Marshal.ReadInt64(co) != -1L) return 1;
            Marshal.FreeCoTaskMem(co);

            // 2. Copies entre tableaux et mémoire native.
            IntPtr b = Marshal.AllocHGlobal(64);
            Marshal.Copy(new byte[] { 9, 1, 2, 3, 4, 9 }, 1, b, 4);
            if (Marshal.ReadInt32(b) != 0x04030201) return 2;
            Marshal.Copy(new int[] { 7, -8, 9 }, 0, b, 3);
            int[] lus = new int[5];
            Marshal.Copy(b, lus, 1, 3);
            if (lus[0] != 0 || lus[1] != 7 || lus[2] != -8 || lus[3] != 9) return 2;
            double[] d = new double[1];
            Marshal.Copy(new double[] { 2.5 }, 0, b, 1);
            Marshal.Copy(b, d, 0, 1);
            if (d[0] != 2.5) return 2;
            try { Marshal.Copy(new byte[2], 1, b, 2); return 2; } catch (ArgumentOutOfRangeException) { }
            try { Marshal.Copy((byte[])null, 0, b, 1); return 2; } catch (ArgumentNullException) { }
            Marshal.FreeHGlobal(b);

            // 3. Chaînes natives.
            IntPtr u = Marshal.StringToHGlobalUni("Bonjour");
            if (Marshal.ReadInt16(u, 14) != 0 || Marshal.PtrToStringUni(u) != "Bonjour" || Marshal.PtrToStringUni(u, 3) != "Bon" || Marshal.PtrToStringAuto(u) != "Bonjour") return 3;
            Marshal.FreeHGlobal(u);
            IntPtr a = Marshal.StringToHGlobalAnsi("abc");
            if (Marshal.ReadByte(a, 0) != (byte)'a' || Marshal.ReadByte(a, 3) != 0 || Marshal.PtrToStringAnsi(a) != "abc" || Marshal.PtrToStringAnsi(a, 2) != "ab") return 3;
            Marshal.FreeHGlobal(a);
            IntPtr e = Marshal.StringToCoTaskMemAnsi("héllo");
            if (Marshal.PtrToStringAnsi(e) != "héllo") return 3;
            Marshal.FreeCoTaskMem(e);
            IntPtr s = Marshal.StringToBSTR("xyz");
            if (Marshal.ReadInt32(s, -4) != 6 || Marshal.PtrToStringBSTR(s) != "xyz") return 3;
            Marshal.FreeBSTR(s);
            if (Marshal.PtrToStringAnsi(IntPtr.Zero) != null || Marshal.PtrToStringUni(IntPtr.Zero) != null || Marshal.StringToHGlobalUni(null) != IntPtr.Zero) return 3;

            // 4. StructureToPtr : disposition native.
            if (Marshal.SizeOf(typeof(Fiche)) != 64 || Marshal.SizeOf(typeof(FicheAnsi)) != 24) return 4;
            if (Marshal.OffsetOf(typeof(Fiche), "Nom") != new IntPtr(24) || Marshal.OffsetOf<Fiche>("P") != new IntPtr(40) || Marshal.OffsetOf(typeof(Fiche), "C") != new IntPtr(58)) return 4;
            Fiche f = new Fiche();
            f.Id = 42; f.Actif = true; f.Code = "ABCDEFGHIJ"; f.Nom = "Corsair";
            f.Valeurs = new short[] { 1, -2, 3 }; f.P.X = 5; f.P.Y = 6; f.D = 1.25; f.Court = true; f.C = 'é';
            IntPtr p = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(Fiche)));
            Marshal.StructureToPtr(f, p, false);
            if (Marshal.ReadInt32(p, 0) != 42 || Marshal.ReadInt32(p, 4) != 1 || Marshal.ReadByte(p, 56) != 1 || Marshal.ReadInt16(p, 58) != 'é') return 4;
            // ByValTStr : 7 caractères et le zéro final.
            if (Marshal.PtrToStringUni(new IntPtr(p.ToInt64() + 8)) != "ABCDEFG") return 4;
            if (Marshal.PtrToStringUni(Marshal.ReadIntPtr(p, 24)) != "Corsair") return 4;
            if (Marshal.ReadInt16(p, 32) != 1 || Marshal.ReadInt16(p, 34) != -2 || Marshal.ReadInt16(p, 36) != 3) return 4;
            if (Marshal.ReadInt32(p, 40) != 5 || Marshal.ReadInt32(p, 44) != 6 || BitConverter.Int64BitsToDouble(Marshal.ReadInt64(p, 48)) != 1.25) return 4;

            // 5. PtrToStructure : retour vers le géré.
            Fiche g = Marshal.PtrToStructure<Fiche>(p);
            if (g.Id != 42 || !g.Actif || g.Code != "ABCDEFG" || g.Nom != "Corsair" || g.Valeurs == null || g.Valeurs.Length != 3 || g.Valeurs[1] != -2) return 5;
            if (g.P.X != 5 || g.P.Y != 6 || g.D != 1.25 || !g.Court || g.C != 'é') return 5;
            Fiche h = (Fiche)Marshal.PtrToStructure(p, typeof(Fiche));
            if (h.Nom != "Corsair" || h.P.Y != 6) return 5;
            // fDeleteOld libère l'ancienne chaîne avant d'écrire la nouvelle.
            f.Nom = "iCUE";
            Marshal.StructureToPtr(f, p, true);
            if (Marshal.PtrToStringUni(Marshal.ReadIntPtr(p, 24)) != "iCUE") return 5;
            Marshal.DestroyStructure(p, typeof(Fiche));
            Marshal.FreeHGlobal(p);

            // 6. Chaîne ANSI et classe à disposition séquentielle.
            FicheAnsi fa = new FicheAnsi();
            fa.C = 'Q'; fa.S = "ansi"; fa.B = 7;
            IntPtr q = Marshal.AllocCoTaskMem(24);
            Marshal.StructureToPtr(fa, q, false);
            if (Marshal.ReadByte(q, 0) != (byte)'Q' || Marshal.PtrToStringAnsi(Marshal.ReadIntPtr(q, 8)) != "ansi" || Marshal.ReadByte(q, 16) != 7) return 6;
            FicheAnsi fb = Marshal.PtrToStructure<FicheAnsi>(q);
            if (fb.C != 'Q' || fb.S != "ansi" || fb.B != 7) return 6;
            Marshal.DestroyStructure<FicheAnsi>(q);
            Boite boite = new Boite();
            boite.A = 3; boite.S = "boite";
            Marshal.StructureToPtr(boite, q, false);
            Boite autre = new Boite();
            Marshal.PtrToStructure(q, autre);
            if (autre.A != 3 || autre.S != "boite") return 6;
            Boite neuve = (Boite)Marshal.PtrToStructure(q, typeof(Boite));
            if (neuve == null || neuve.A != 3 || neuve.S != "boite") return 6;
            Marshal.FreeCoTaskMem(q);

            // 7. Refus documentés.
            try { Marshal.StructureToPtr(null, new IntPtr(8), false); return 7; } catch (ArgumentNullException) { }
            try { Marshal.OffsetOf(typeof(Fiche), "Absent"); return 7; } catch (ArgumentException) { }
            try { Marshal.PtrToStructure(new IntPtr(8), (object)new Point()); return 7; } catch (ArgumentException) { }
            if (Marshal.PtrToStructure(IntPtr.Zero, typeof(Fiche)) != null) return 7;

            // 8. HRESULT.
            if (!(Marshal.GetExceptionForHR(unchecked((int)0x80070057)) is ArgumentException)) return 8;
            COMException com = Marshal.GetExceptionForHR(unchecked((int)0x80004005)) as COMException;
            if (com == null || com.ErrorCode != unchecked((int)0x80004005)) return 8;
            if (Marshal.GetExceptionForHR(0) != null || Marshal.GetExceptionForHR(unchecked((int)0x80004001)).GetType().Name != "NotImplementedException") return 8;
            Marshal.ThrowExceptionForHR(0);
            try { Marshal.ThrowExceptionForHR(unchecked((int)0x80070005)); return 8; } catch (UnauthorizedAccessException) { }
            if (Marshal.GetHRForException(com) != unchecked((int)0x80004005)) return 8;
            return 200;
        }
    }
}
