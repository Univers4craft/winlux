// Témoin de DateTime, DateTimeOffset, TimeSpan.ToString et
// System.Text.Encoding dans la mscorlib Winlux (#1963).
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 14) de la première vérification qui échoue.
// Corsair.Service.exe (iCUE 4.18) appelait DateTime::get_Now,
// DateTime::op_Subtraction, Encoding::GetBytes et Encoding::get_UTF8.
using System;
using System.Text;

namespace Temoins
{
    static class Dates
    {
        static int Main()
        {
            DateTime d = new DateTime(2024, 2, 29, 13, 45, 30, 250);
            if (d.Year != 2024 || d.Month != 2 || d.Day != 29 || d.Hour != 13 || d.Minute != 45 || d.Second != 30 || d.Millisecond != 250) return 1;
            if (d.DayOfWeek != DayOfWeek.Thursday || d.DayOfYear != 60 || new DateTime(1, 1, 1).DayOfWeek != DayOfWeek.Monday) return 2;
            if (d.AddDays(1).Month != 3 || d.AddMonths(12).Day != 28 || d.AddYears(-1).Day != 28 || d.AddMinutes(20).Hour != 14) return 3;
            TimeSpan ecart = new DateTime(2024, 3, 1) - new DateTime(2024, 2, 1);
            if (ecart.Days != 29 || (d - d.AddSeconds(-90)).TotalSeconds != 90) return 4;
            if (d.ToString("yyyy-MM-dd HH:mm:ss.fff") != "2024-02-29 13:45:30.250" || d.ToString() != "02/29/2024 13:45:30") return 5;
            if (d.ToString("dddd d MMMM yy, h:mm tt") != "Thursday 29 February 24, 1:45 PM" || d.ToString("s") != "2024-02-29T13:45:30") return 6;
            DateTime u = new DateTime(2026, 11, 11, 20, 0, 0, DateTimeKind.Utc);
            if (u.ToString("o") != "2026-11-11T20:00:00.0000000Z" || u.ToString("u") != "2026-11-11 20:00:00Z") return 7;
            DateTime lu = DateTime.Parse("2026-11-11T20:30:15");
            if (lu.Hour != 20 || lu.Minute != 30 || lu.Second != 15 || DateTime.Parse("11/11/2026").Day != 11) return 8;
            DateTime now = DateTime.Now;
            DateTime utc = DateTime.UtcNow;
            if (now.Year < 2024 || utc.Kind != DateTimeKind.Utc || (now.ToUniversalTime() - utc).Duration().TotalSeconds > 5) return 9;
            DateTimeOffset o = new DateTimeOffset(2026, 11, 11, 21, 0, 0, TimeSpan.FromHours(1));
            if (o.UtcDateTime.Hour != 20 || o.ToUnixTimeSeconds() != 1794427200 || DateTimeOffset.FromUnixTimeSeconds(0).Year != 1970) return 10;
            if (o.ToString("yyyy-MM-ddTHH:mm:sszzz") != "2026-11-11T21:00:00+01:00" || DateTimeOffset.Parse("2026-11-11T21:00:00+01:00") != o) return 11;
            if (new TimeSpan(1, 2, 3, 4).ToString() != "1.02:03:04" || TimeSpan.FromMilliseconds(1500).ToString() != "00:00:01.5000000") return 12;

            byte[] utf8 = Encoding.UTF8.GetBytes("é€😀");
            if (utf8.Length != 9 || utf8[0] != 0xC3 || utf8[2] != 0xE2 || utf8[5] != 0xF0 || Encoding.UTF8.GetString(utf8) != "é€😀") return 13;
            if (Encoding.ASCII.GetString(Encoding.ASCII.GetBytes("aé")) != "a?" || Encoding.Unicode.GetBytes("A")[0] != 65
                || Encoding.GetEncoding(1252).GetBytes("€")[0] != 0x80 || Encoding.UTF8.GetPreamble().Length != 3
                || Encoding.UTF8.GetString(new byte[] { 0x61, 0xFF, 0x62 }) != "a�b" || Encoding.UTF8.GetByteCount("ab") != 2) return 14;
            return 200;
        }
    }
}
