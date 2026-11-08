// Témoin de l'étape 7 : un programme C# compilé contre la mscorlib Winlux.
// Code de sortie attendu : 15.
// String.IsNullOrWhiteSpace et Char.IsWhiteSpace s'exécutent depuis notre
// mscorlib (code C#) ; Console.WriteLine reste servi par le moteur.
using System;

static class Bonjour
{
    static int Main()
    {
        int code = 0;
        if (string.IsNullOrWhiteSpace(" \t\n")) code += 1;
        if (!string.IsNullOrWhiteSpace(" x ")) code += 2;
        if (string.IsNullOrEmpty("")) code += 4;
        if (char.IsWhiteSpace((char)0x3000)) code += 8;
        Console.WriteLine("Bonjour depuis la BCL Winlux");
        return code;
    }
}
