// Témoin de CultureInfo.DefaultThreadCurrentCulture et
// DefaultThreadCurrentUICulture dans la mscorlib Winlux (#2279).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 3) de la première vérification qui échoue.
// lghub_setup.exe (installeur de Logitech G HUB) écrivait
// CultureInfo::set_DefaultThreadCurrentCulture dès son démarrage.
using System;
using System.Globalization;

namespace Temoins
{
    static class Cultures
    {
        static int Main()
        {
            // 1. Rien d'écrit : null.
            if (CultureInfo.DefaultThreadCurrentCulture != null || CultureInfo.DefaultThreadCurrentUICulture != null) return 1;

            // 2. La valeur écrite se relit et devient la culture courante.
            CultureInfo us = new CultureInfo("en-US");
            CultureInfo fr = new CultureInfo("fr-FR");
            CultureInfo.DefaultThreadCurrentCulture = us;
            CultureInfo.DefaultThreadCurrentUICulture = fr;
            if (!object.ReferenceEquals(CultureInfo.DefaultThreadCurrentCulture, us) || !object.ReferenceEquals(CultureInfo.DefaultThreadCurrentUICulture, fr)) return 2;
            if (CultureInfo.CurrentCulture.Name != "en-US" || CultureInfo.CurrentUICulture.Name != "fr-FR") return 2;

            // 3. null rétablit le comportement d'avant.
            CultureInfo.DefaultThreadCurrentCulture = null;
            CultureInfo.DefaultThreadCurrentUICulture = null;
            if (CultureInfo.DefaultThreadCurrentCulture != null || CultureInfo.DefaultThreadCurrentUICulture != null) return 3;
            if (!object.ReferenceEquals(CultureInfo.CurrentCulture, CultureInfo.InvariantCulture)) return 3;
            return 200;
        }
    }
}
