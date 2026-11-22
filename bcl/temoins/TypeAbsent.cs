// Témoin de isinst/castclass vers un type que le moteur ne connaît pas (#2790).
// Façade de compilation : System.Numerics.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 3) de la première vérification qui échoue.
// Newtonsoft.Json teste « valeur is BigInteger » avant de convertir un
// JToken : le moteur répondait oui pour tout objet, et le téléchargeur de
// Paint.NET mourait sur BigInteger::op_Explicit pour un simple entier.
extern alias Facade;
using System;
using System.Collections.Generic;
using BigInteger = Facade::System.Numerics.BigInteger;
using Absente = Facade::System.Numerics.Absente;

static class TypeAbsent
{
    static int Main()
    {
        // 1. Un objet de type connu n'est pas une instance d'un type absent.
        object[] connus = { 5L, 3, "texte", new List<int>(), new object(), 2.5 };
        foreach (object o in connus)
            if (o is BigInteger || o is Absente) return 1;

        // 2. Le cas de Newtonsoft : la branche BigInteger n'est pas prise.
        object v = 42L;
        int n = v is BigInteger ? (int)(BigInteger)v : Convert.ToInt32(v);
        if (n != 42) return 2;

        // 3. castclass vers un type absent refuse un objet connu.
        try { Absente a = (Absente)(object)"x"; return 3; } catch (InvalidCastException) { }

        return 200;
    }
}
