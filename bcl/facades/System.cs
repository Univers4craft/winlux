// Façade de compilation « System » pour les témoins de la BCL Winlux (#1947).
// Licence GPL 3. Elle n'est jamais chargée ni livrée : un témoin compilé
// contre elle référence ses types dans l'assembly « System », comme un
// programme .NET Framework, et le moteur doit alors les trouver dans notre
// mscorlib (System.Diagnostics.Stopwatch est dans System.dll).

[assembly: System.Reflection.AssemblyVersion("4.0.0.0")]

namespace System.Diagnostics
{
    public class Stopwatch
    {
        public static Stopwatch StartNew() { throw new InvalidOperationException("façade"); }
        public void Stop() { throw new InvalidOperationException("façade"); }
        public long ElapsedTicks { get { throw new InvalidOperationException("façade"); } }
        public bool IsRunning { get { throw new InvalidOperationException("façade"); } }
    }
}
