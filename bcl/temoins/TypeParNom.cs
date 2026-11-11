// Témoin de Type.GetType(string…) dans la mscorlib Winlux (#1884).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait Type.GetType après Type.Assembly.
using System;

namespace Temoins
{
    class Externe
    {
        public class Interne { }
    }

    static class TypeParNom
    {
        static int Main()
        {
            if (Type.GetType("Temoins.Externe") != typeof(Externe)) return 1;
            if (Type.GetType("Temoins.Externe+Interne") != typeof(Externe.Interne)) return 2;
            if (Type.GetType("Temoins.Externe, TypeParNom, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null") != typeof(Externe)) return 3;
            if (Type.GetType("System.TimeSpan") != typeof(TimeSpan)) return 4;
            if (Type.GetType("System.String, mscorlib") != typeof(string)) return 5;
            if ((object)Type.GetType("Temoins.Absent") != null) return 6;
            if ((object)Type.GetType("Temoins.Externe, Assembly.Absente", false) != null) return 7;
            if (Type.GetType("temoins.EXTERNE", false, true) != typeof(Externe)) return 8;
            try
            {
                Type.GetType("Temoins.Absent", true);
                return 9;
            }
            catch (TypeLoadException)
            {
            }
            return 200;
        }
    }
}
