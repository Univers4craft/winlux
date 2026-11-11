// Témoin de System.TimeSpan dans la mscorlib Winlux (#1686).
// Code de sortie attendu : 127.
// Corsair.Service.exe (iCUE 4.18) appelait TimeSpan.FromMilliseconds.
using System;

static class Durees
{
    static int Main()
    {
        int code = 0;
        TimeSpan a = TimeSpan.FromMilliseconds(1500.4);
        if (a.Ticks == 15000000 && a.Seconds == 1 && a.Milliseconds == 500) code += 1;
        if (TimeSpan.FromSeconds(90).TotalMinutes == 1.5) code += 2;
        TimeSpan b = new TimeSpan(1, 2, 3, 4, 5);
        if (b.Days == 1 && b.Hours == 2 && b.Minutes == 3 && b.Seconds == 4 && b.Milliseconds == 5) code += 4;
        if (b - TimeSpan.FromDays(1) == new TimeSpan(2, 3, 4) + TimeSpan.FromTicks(50000)) code += 8;
        if (TimeSpan.FromMinutes(-1).Duration() > TimeSpan.Zero && TimeSpan.Compare(a, b) == -1) code += 16;
        try
        {
            TimeSpan.FromDays(1e10);
        }
        catch (OverflowException)
        {
            code += 32;
        }
        object boite = a;
        if (boite.Equals(TimeSpan.FromTicks(15000000)) && !a.Equals(b)) code += 64;
        return code;
    }
}
