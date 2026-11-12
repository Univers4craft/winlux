// Témoin de Type.Assembly et de System.Reflection.Assembly dans la
// mscorlib Winlux (#1869).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 12) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) lisait Type.Assembly au démarrage.
using System;
using System.Reflection;

[assembly: AssemblyVersion("1.2.3.4")]

namespace Temoins
{
    class Base { }
    class Derivee : Base { }

    static class Reflexion
    {
        static int Main()
        {
            Assembly a = typeof(Derivee).Assembly;
            if (!((object)a != null)) return 1;
            if (!(a == Assembly.GetExecutingAssembly())) return 2;
            if (!(a == Assembly.GetEntryAssembly())) return 3;
            if (!(a.FullName == "Reflexion, Version=1.2.3.4, Culture=neutral, PublicKeyToken=null")) return 4;
            AssemblyName n = a.GetName();
            if (!(n.Name == "Reflexion" && n.Version == new Version(1, 2, 3, 4))) return 5;
            if (!(n.Version.ToString() == "1.2.3.4" && n.Version > new Version(1, 2))) return 6;
            if (!(a.Location.EndsWith("Reflexion.exe"))) return 7;
            Assembly corlib = typeof(string).Assembly;
            if (!(corlib.GetName().Name == "mscorlib" && corlib != a)) return 8;
            if (!(typeof(Derivee) == typeof(Derivee) && typeof(Derivee) != typeof(Base))) return 9;
            if (!(typeof(Derivee).Name == "Derivee" && typeof(Derivee).FullName == "Temoins.Derivee"
                && typeof(Derivee).Namespace == "Temoins")) return 10;
            if (!(typeof(Derivee).BaseType == typeof(Base) && new Derivee().GetType() == typeof(Derivee))) return 11;
            if (!(typeof(Derivee).IsClass && !typeof(Derivee).IsValueType && typeof(int).IsValueType)) return 12;
            return 200;
        }
    }
}
