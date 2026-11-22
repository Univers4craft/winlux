// Témoin des tableaux à plusieurs dimensions (#2848).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// `string[,]` s'arrêtait sur « Array::Set introuvable » : le moteur ne
// fournissait ni le constructeur ni Get, Set et Address d'un type tableau
// (ECMA-335 §II.14.2), et la signature ARRAY laissait sa forme non lue.
using System;

namespace Temoins
{
    struct Paire
    {
        public int A;
        public long B;
    }

    static class TableauxMultiples
    {
        // Signature avec ARRAY suivi d'un autre paramètre : la forme du
        // tableau doit être lue en entier pour trouver `decalage`.
        static int Somme(int[,] t, int decalage)
        {
            int s = 0;
            for (int i = 0; i < t.GetLength(0); i++)
                for (int j = 0; j < t.GetLength(1); j++)
                    s += t[i, j];
            return s + decalage;
        }

        static void Doubler(ref int x) { x *= 2; }

        static int Main()
        {
            // 1. string[,] : Set, Get, Rank, Length, GetLength.
            string[,] noms = new string[2, 3];
            noms[1, 2] = "fin";
            noms[0, 0] = "début";
            if (noms[1, 2] != "fin" || noms[0, 0] != "début" || noms[0, 1] != null) return 1;
            if (noms.Rank != 2 || noms.Length != 6 || noms.GetLength(0) != 2 || noms.GetLength(1) != 3) return 1;

            // 2. int[,,] : rangement ligne par ligne, Address (ref) et une
            //    méthode qui prend le tableau puis un entier.
            int[,,] cube = new int[2, 3, 4];
            int n = 0;
            for (int i = 0; i < 2; i++)
                for (int j = 0; j < 3; j++)
                    for (int k = 0; k < 4; k++)
                        cube[i, j, k] = n++;
            if (cube[1, 2, 3] != 23 || cube[0, 1, 0] != 4 || cube.Rank != 3 || cube.Length != 24) return 2;
            Doubler(ref cube[1, 0, 0]);
            cube[0, 0, 1] += 10;
            if (cube[1, 0, 0] != 24 || cube[0, 0, 1] != 11) return 2;
            if (Somme(new int[,] { { 1, 2 }, { 3, 4 } }, 100) != 110) return 2;

            // 3. Bornes : GetLowerBound / GetUpperBound, et GetLength d'une
            //    dimension absente.
            if (noms.GetLowerBound(1) != 0 || noms.GetUpperBound(0) != 1 || noms.GetUpperBound(1) != 2) return 3;
            try { noms.GetLength(2); return 3; } catch (IndexOutOfRangeException) { }

            // 4. Hors bornes : IndexOutOfRangeException.
            try { noms[2, 0] = "x"; return 4; } catch (IndexOutOfRangeException) { }
            try { string s = noms[0, -1]; return 4; } catch (IndexOutOfRangeException) { }

            // 5. Structures en éléments.
            Paire[,] paires = new Paire[2, 2];
            paires[1, 1].A = 7;
            paires[1, 1].B = 1L << 40;
            Paire p = paires[1, 1];
            if (p.A != 7 || p.B != 1L << 40 || paires[0, 1].A != 0) return 5;

            // 6. GetValue / SetValue à plusieurs indices.
            noms.SetValue("milieu", 0, 1);
            if ((string)noms.GetValue(0, 1) != "milieu" || (string)noms.GetValue(new int[] { 1, 2 }) != "fin") return 6;
            cube.SetValue(99, 1, 1, 1);
            if (cube[1, 1, 1] != 99 || (int)cube.GetValue(1L, 1L, 1L) != 99) return 6;
            try { noms.GetValue(0); return 6; } catch (ArgumentException) { }
            try { noms.GetValue(5, 0); return 6; } catch (IndexOutOfRangeException) { }

            // 7. Array.CreateInstance à plusieurs dimensions.
            Array a = Array.CreateInstance(typeof(int), 3, 2);
            if (a.Rank != 2 || a.Length != 6 || a.GetLength(0) != 3) return 7;
            a.SetValue(5, 2, 1);
            int[,] vue = (int[,])a;
            if (vue[2, 1] != 5) return 7;
            Array b = Array.CreateInstance(typeof(string), new int[] { 2, 2 }, new int[] { 1, 10 });
            b.SetValue("x", 2, 11);
            if (b.GetLowerBound(1) != 10 || b.GetUpperBound(1) != 11 || (string)b.GetValue(2, 11) != "x") return 7;
            try { b.GetValue(0, 10); return 7; } catch (IndexOutOfRangeException) { }

            // 8. Copie à plat (ligne par ligne, comme .NET) et Clone.
            int[] plat = new int[6];
            Array.Copy(new int[,] { { 1, 2, 3 }, { 4, 5, 6 } }, plat, 6);
            for (int i = 0; i < 6; i++) if (plat[i] != i + 1) return 8;
            string[,] copie = (string[,])noms.Clone();
            copie[1, 2] = "autre";
            if (copie.Rank != 2 || copie[0, 1] != "milieu" || noms[1, 2] != "fin") return 8;

            // 9. Un tableau à une dimension reste de rang 1.
            int[] simple = new int[4];
            if (simple.Rank != 1 || simple.GetUpperBound(0) != 3 || (int)simple.GetValue(2) != 0) return 9;
            try { simple.GetValue(0, 0); return 9; } catch (ArgumentException) { }

            return 200;
        }
    }
}
