// Témoin de Comparer<T>, String, StringBuilder, des primitives (ToString,
// Parse, TryParse) et de Char dans la mscorlib Winlux (#1955, #1956).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 24) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait Comparer`1::get_Default,
// String::Format, StringBuilder::Append, Int32::ToString et Int32::TryParse.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace Temoins
{
    class Version : IComparable<Version>
    {
        public int N;
        public Version(int n) { N = n; }
        public int CompareTo(Version autre) { return N.CompareTo(autre.N); }
    }

    static class Chaines
    {
        static int Main()
        {
            if (Comparer<int>.Default.Compare(3, 5) >= 0 || Comparer<int>.Default.Compare(5, 5) != 0) return 1;
            if (Comparer<string>.Default.Compare("b", "a") <= 0 || Comparer<string>.Default.Compare(null, "a") >= 0) return 2;
            List<Version> vs = new List<Version> { new Version(3), new Version(1), new Version(2) };
            vs.Sort();
            List<int> ns = new List<int> { 5, 2, 9, 1 };
            ns.Sort();
            if (vs[0].N != 1 || vs[2].N != 3 || ns[0] != 1 || ns[3] != 9 || ns.BinarySearch(5) != 2) return 3;

            if (new string('x', 3) != "xxx" || new string(new char[] { 'a', 'b', 'c' }, 1, 2) != "bc") return 4;
            string e = "été";
            if (e.Length != 3 || e[1] != 't' || e.IndexOf('t') != 1 || e.Substring(1) != "té") return 5;
            if (string.Format("{0}-{1:X2}-{2,4}|{3,-3}|{{x}}", "a", 255, 7, 1) != "a-FF-   7|1  |{x}") return 6;
            if (string.Format("{0:F2} {1:N0} {2:D4} {3:0.0}", 3.14159, 1234567, 42, 2.25) != "3.14 1,234,567 0042 2.3") return 7;
            string[] parts = "a,b,,c".Split(',');
            string[] sans = "a,b,,c".Split(new char[] { ',' }, StringSplitOptions.RemoveEmptyEntries);
            string[] mots = "un--deux--trois".Split(new string[] { "--" }, StringSplitOptions.None);
            if (parts.Length != 4 || parts[2] != "" || sans.Length != 3 || mots[1] != "deux" || mots.Length != 3) return 8;
            if (string.Join("+", new string[] { "a", "b" }) != "a+b" || string.Join(", ", new List<int> { 1, 2 }) != "1, 2") return 9;
            if ("  xy ".Trim() != "xy" || "--xy-".Trim('-') != "xy" || "xyy".TrimEnd('y') != "x" || "007".TrimStart('0') != "7") return 10;
            if ("Bonjour".IndexOf("jour") != 3 || "abcabc".LastIndexOf('b') != 4 || "abc".IndexOf("C", StringComparison.OrdinalIgnoreCase) != 2) return 11;
            if (!"Fichier.TXT".EndsWith(".txt", StringComparison.OrdinalIgnoreCase) || !string.Equals("ABC", "abc", StringComparison.OrdinalIgnoreCase)) return 12;
            if ("7".PadLeft(3, '0') != "007" || "ab".Insert(1, "X") != "aXb" || "abcd".Remove(1, 2) != "ad" || "a.b".Replace('.', '/') != "a/b") return 13;
            if (string.Compare("a", "B") >= 0 || string.CompareOrdinal("a", "B") <= 0 || "abc".ToUpperInvariant() != "ABC") return 14;

            StringBuilder sb = new StringBuilder();
            sb.Append("n=").Append(42).Append(' ').Append(true).AppendFormat("[{0:x}]", 255);
            sb.Insert(0, ">").Replace("n", "N");
            if (sb.ToString() != ">N=42 True[ff]" || sb.Length != 14) return 15;
            sb.Remove(0, 1).Clear().Append('z', 2);
            if (sb.ToString() != "zz") return 16;

            if (42.ToString() != "42" || (-5).ToString("D3") != "-005" || 255.ToString("X4") != "00FF" || (-1).ToString("X") != "FFFFFFFF") return 17;
            if (((byte)200).ToString() != "200" || ((long)1 << 40).ToString() != "1099511627776" || ulong.MaxValue.ToString() != "18446744073709551615") return 18;
            if ((0.1 + 0.2).ToString() != "0.3" || 1.5.ToString() != "1.5" || (1e20).ToString() != "1E+20" || 2.5f.ToString() != "2.5") return 19;
            int n;
            if (int.Parse("-123") != -123 || !int.TryParse(" 77 ", out n) || n != 77 || int.TryParse("12a", out n) || n != 0) return 20;
            if (int.Parse("ff", NumberStyles.HexNumber) != 255 || int.Parse("FFFFFFFF", NumberStyles.HexNumber) != -1) return 21;
            try { int.Parse("99999999999"); return 22; } catch (OverflowException) { }
            try { int.Parse("abc"); return 22; } catch (FormatException) { }
            double d;
            if (double.Parse("3.5") != 3.5 || !double.TryParse("-2e3", out d) || d != -2000 || bool.Parse(" true ") != true) return 23;
            if (char.ToUpper('é') != 'É' || !char.IsLetter('ß') || char.IsLetter('1') || !char.IsPunctuation('!') || 'a'.ToString() != "a") return 24;
            object boite = 12;
            if (boite.ToString() != "12" || string.Concat("x", 3.5, true) != "x3.5True") return 24;
            return 200;
        }
    }
}
