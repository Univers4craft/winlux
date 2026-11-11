// Témoin de System.Array, BitConverter, Guid et Convert dans la mscorlib
// Winlux (#1962).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 16) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait Array::Copy (18 fois),
// BitConverter::GetBytes, Guid::NewGuid et Convert::ToInt32.
using System;
using System.Collections.Generic;

namespace Temoins
{
    struct Point
    {
        public int X;
        public long Y;
    }

    static class Tableaux
    {
        static int Main()
        {
            int[] a = { 1, 2, 3, 4, 5 };
            int[] b = new int[7];
            Array.Copy(a, 1, b, 2, 3);
            if (b[1] != 0 || b[2] != 2 || b[4] != 4 || b[5] != 0) return 1;
            Array.Copy(a, 0, a, 1, 4);
            if (a[0] != 1 || a[1] != 1 || a[4] != 4) return 2;
            string[] s = { "x", "y", "z" };
            string[] t = new string[3];
            s.CopyTo(t, 0);
            Array.Reverse(t);
            if (t[0] != "z" || t[2] != "x") return 3;
            Point[] p = new Point[2];
            p[0].X = 7;
            p[0].Y = 1L << 40;
            Point[] q = (Point[])p.Clone();
            p[0].X = 9;
            if (q[0].X != 7 || q[0].Y != 1L << 40) return 4;
            Array.Clear(a, 0, 2);
            if (a[0] != 0 || a[1] != 0 || a[2] != 2) return 5;
            Array.Resize(ref a, 8);
            if (a.Length != 8 || a[2] != 2 || Array.IndexOf(a, 4) != 4 || Array.IndexOf(s, "z") != 2) return 6;
            int[] tri = { 5, 3, 9, 1 };
            Array.Sort(tri);
            if (tri[0] != 1 || tri[3] != 9 || Array.BinarySearch(tri, 5) != 2) return 7;
            string[] nombres = Array.ConvertAll(tri, n => n.ToString());
            if (nombres[2] != "5" || !Array.Exists(tri, n => n > 8) || Array.Find(tri, n => n > 3) != 5) return 8;
            byte[] octets = (byte[])Array.CreateInstance(typeof(byte), 3);
            if (octets.Length != 3) return 9;

            byte[] g = BitConverter.GetBytes(0x01020304);
            if (g.Length != 4 || g[0] != 4 || g[3] != 1 || BitConverter.ToInt32(g, 0) != 0x01020304) return 10;
            if (BitConverter.ToUInt16(new byte[] { 0xFF, 0x7F }, 0) != 0x7FFF || BitConverter.ToInt64(BitConverter.GetBytes(-2L), 0) != -2) return 11;
            if (BitConverter.ToDouble(BitConverter.GetBytes(1.5), 0) != 1.5 || BitConverter.ToString(new byte[] { 1, 0xAB }) != "01-AB") return 12;

            Guid id = Guid.NewGuid();
            Guid copie = new Guid(id.ToString());
            if (id == Guid.Empty || id != copie || id.ToString().Length != 36 || id.ToString()[14] != '4') return 13;
            Guid fixe = new Guid("0f8fad5b-d9cb-469f-a165-70867728950e");
            if (fixe.ToByteArray()[0] != 0x5b || fixe.ToString("N") != "0f8fad5bd9cb469fa16570867728950e") return 14;

            if (Convert.ToInt32("42") != 42 || Convert.ToInt32(2.5) != 2 || Convert.ToInt32(3.5) != 4 || Convert.ToInt32((object)7L) != 7) return 15;
            if (Convert.ToByte("ff", 16) != 255 || Convert.ToString(255, 2) != "11111111" || Convert.ToString(-1, 16) != "ffffffff") return 16;
            try { Convert.ToByte(300); return 16; } catch (OverflowException) { }
            if (Convert.ToBase64String(new byte[] { 1, 2, 3, 4 }) != "AQIDBA==" || Convert.FromBase64String("AQIDBA==")[3] != 4) return 16;
            if (!Convert.ToBoolean("True") || Convert.ToDouble("2.5") != 2.5 || Convert.ToString(true) != "True") return 16;
            return 200;
        }
    }
}
