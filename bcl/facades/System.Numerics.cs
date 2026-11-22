// Façade de compilation (jamais livrée) : des types de System.Numerics que
// notre mscorlib ne sert pas, pour les témoins qui vérifient le comportement
// du moteur devant un type absent (#2790).
[assembly: System.Reflection.AssemblyVersion("4.0.0.0")]
namespace System.Numerics
{
    public struct BigInteger
    {
        public static explicit operator int(BigInteger value) { return 0; }
    }

    public class Absente
    {
    }
}
