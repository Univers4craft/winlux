// Témoin de System.Math et System.GC dans la mscorlib Winlux (#2077).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 8) de la première vérification qui échoue.
// Chaque valeur attendue est celle de .NET Framework 4.x sous Windows,
// relevée dans le tableau de l'issue #2077.
using System;

namespace Temoins
{
    static class Calcul
    {
        static bool Proche(double a, double b) { return Math.Abs(a - b) < 1e-12; }
        static bool MoinsZero(double d) { return d == 0 && 1 / d < 0; }

        static int Main()
        {
            // 1. Arrondi bancaire.
            if (Math.Round(2.5) != 2 || Math.Round(-2.5) != -2 || Math.Round(3.5) != 4 || Math.Round(0.5) != 0) return 1;
            if (Math.Round(2.4) != 2 || Math.Round(2.6) != 3 || Math.Round(-2.6) != -3 || !MoinsZero(Math.Round(-0.4))) return 1;
            if (!double.IsNaN(Math.Round(double.NaN)) || Math.Round(1e300) != 1e300) return 1;

            // 2. Arrondi à n chiffres et MidpointRounding.
            if (Math.Round(0.125, 2) != 0.12 || Math.Round(1.2345, 3) != 1.234 || Math.Round(123.0, 0) != 123) return 2;
            if (Math.Round(2.5, MidpointRounding.AwayFromZero) != 3 || Math.Round(-2.5, MidpointRounding.AwayFromZero) != -3) return 2;
            if (Math.Round(0.125, 2, MidpointRounding.AwayFromZero) != 0.13 || Math.Round(2.5, MidpointRounding.ToEven) != 2) return 2;
            try { Math.Round(1.0, 16); return 2; } catch (ArgumentOutOfRangeException) { }
            try { Math.Round(1.0, -1); return 2; } catch (ArgumentOutOfRangeException) { }

            // 3. Min et Max gardent l'ordre non signé.
            if (Math.Max(3000000000u, 5u) != 3000000000u || Math.Min(3000000000u, 5u) != 5u) return 3;
            if (Math.Min(18000000000000000000UL, 1UL) != 1UL || Math.Max(18000000000000000000UL, 1UL) != 18000000000000000000UL) return 3;
            if (Math.Max((byte)200, (byte)100) != 200 || Math.Max((ushort)60000, (ushort)1) != 60000) return 3;
            if (Math.Min(-3, 2) != -3 || Math.Max(-3L, 2L) != 2L || Math.Min((sbyte)-1, (sbyte)1) != -1) return 3;

            // 4. NaN l'emporte dans Min et Max.
            if (!double.IsNaN(Math.Max(double.NaN, 1.0)) || !double.IsNaN(Math.Max(1.0, double.NaN))) return 4;
            if (!double.IsNaN(Math.Min(double.NaN, 1.0)) || !double.IsNaN(Math.Min(1.0, double.NaN))) return 4;
            if (!float.IsNaN(Math.Max(float.NaN, 1f)) || Math.Max(1.5, 2.5) != 2.5 || Math.Min(1.5f, 2.5f) != 1.5f) return 4;

            // 5. Abs : le minimum d'un entier signé déborde.
            try { Math.Abs(int.MinValue); return 5; } catch (OverflowException) { }
            try { Math.Abs(long.MinValue); return 5; } catch (OverflowException) { }
            try { Math.Abs(short.MinValue); return 5; } catch (OverflowException) { }
            if (Math.Abs(-7) != 7 || Math.Abs(-7L) != 7L || Math.Abs(-1.5) != 1.5 || MoinsZero(Math.Abs(-0.0))) return 5;

            // 6. Sign, et NaN refusé.
            if (Math.Sign(-4) != -1 || Math.Sign(0L) != 0 || Math.Sign(2.5) != 1 || Math.Sign(-0.0) != 0) return 6;
            try { Math.Sign(double.NaN); return 6; } catch (ArithmeticException) { }

            // 7. Fonctions servies par le moteur et cas documentés.
            if (Math.Floor(-1.5) != -2 || Math.Ceiling(-1.5) != -1 || Math.Truncate(-1.5) != -1 || Math.Sqrt(16) != 4) return 7;
            if (!Proche(Math.Log(8, 2), 3) || !double.IsNaN(Math.Log(5, 1)) || !Proche(Math.Pow(2, 10), 1024)) return 7;
            if (!Proche(Math.Cosh(0), 1) || !Proche(Math.Tanh(0), 0) || !Proche(Math.Atan2(1, 1), Math.PI / 4)) return 7;
            if (Math.IEEERemainder(3, 2) != -1 || Math.IEEERemainder(10, 3) != 1 || !double.IsNaN(Math.IEEERemainder(1, 0))) return 7;
            int reste;
            if (Math.DivRem(17, 5, out reste) != 3 || reste != 2 || Math.BigMul(int.MaxValue, 2) != 4294967294L) return 7;

            // 8. GC.
            try { GC.SuppressFinalize(null); return 8; } catch (ArgumentNullException) { }
            GC.SuppressFinalize(new object());
            GC.KeepAlive(new object());
            GC.Collect();
            GC.Collect(0, GCCollectionMode.Forced);
            GC.WaitForPendingFinalizers();
            try { GC.Collect(-1); return 8; } catch (ArgumentOutOfRangeException) { }
            if (GC.MaxGeneration != 2 || GC.GetTotalMemory(false) <= 0 || GC.GetGeneration(new object()) != 0) return 8;
            return 200;
        }
    }
}
