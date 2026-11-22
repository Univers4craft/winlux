// Témoin de System.IO.Compression (#2819).
// Façade de compilation : System.IO.Compression.
// Code de sortie attendu : 200.
// Sinon : le numéro (1 à 9) de la première vérification qui échoue.
// Le téléchargeur de Paint.NET ouvre son paquet avec new ZipArchive(flux),
// parcourt Entries et lit chaque entrée par Open() et Name : notre mscorlib
// n'avait aucun type de System.IO.Compression.
//
// L'archive ci-dessous a été écrite par le module zipfile de Python : une
// entrée Deflate à codes dynamiques, une à codes fixes, une stockée, une
// Deflate de niveau 0 (blocs stockés), un nom UTF-8 et un dossier.
// WINLUX_TEMOIN_ZIP peut nommer une grosse archive (scripts/test-zip.sh) :
// chaque entrée y est comparée au fichier de même nom dans « <archive>.d/ ».
extern alias Facade;
using System;
using System.IO;
using ZipArchive = Facade::System.IO.Compression.ZipArchive;
using ZipArchiveEntry = Facade::System.IO.Compression.ZipArchiveEntry;
using ZipArchiveMode = Facade::System.IO.Compression.ZipArchiveMode;

static class Compression
{
    const string Archive = "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";

    static byte[] Lire(Stream s)
    {
        MemoryStream m = new MemoryStream();
        byte[] b = new byte[333];
        int n;
        while ((n = s.Read(b, 0, b.Length)) > 0) m.Write(b, 0, n);
        return m.ToArray();
    }

    static string Texte(ZipArchiveEntry e)
    {
        using (Stream s = e.Open()) return System.Text.Encoding.UTF8.GetString(Lire(s));
    }

    static bool Egal(byte[] a, byte[] b)
    {
        if (a.Length != b.Length) return false;
        for (int i = 0; i < a.Length; i++) if (a[i] != b[i]) return false;
        return true;
    }

    static int Main()
    {
        byte[] octets = Convert.FromBase64String(Archive);
        using (ZipArchive z = new ZipArchive(new MemoryStream(octets)))
        {
            // 1. Les six entrées, dans l'ordre du répertoire central.
            if (z.Entries.Count != 6) return 1;
            if (z.Entries[0].FullName != "dossier/texte.txt" || z.Entries[5].FullName != "dossier/") return 1;

            // 2. Name : après le dernier « / » ; vide pour un dossier.
            if (z.Entries[0].Name != "texte.txt" || z.Entries[5].Name != "") return 2;

            // 3. Deflate à codes dynamiques : le texte entier, et Length.
            System.Text.StringBuilder sb = new System.Text.StringBuilder();
            for (int i = 0; i < 100; i++) sb.Append("ligne ").Append(i).Append(" : ").Append(i * i).Append('\n');
            string attendu = sb.ToString();
            if (Texte(z.Entries[0]) != attendu || z.Entries[0].Length != attendu.Length) return 3;

            // 4. Deflate à codes fixes.
            if (Texte(z.GetEntry("petit.txt")) != "Bonjour") return 4;

            // 5. Entrée stockée (méthode 0), puis Deflate de niveau 0.
            byte[] brut = Lire(z.GetEntry("brut.bin").Open());
            if (brut.Length != 40 || brut[39] != 39) return 5;
            byte[] n0 = Lire(z.GetEntry("niveau0.bin").Open());
            if (n0.Length != 300) return 5;
            for (int i = 0; i < 300; i++) if (n0[i] != (byte)((i * 7) & 0xFF)) return 5;

            // 6. Nom UTF-8 (bit 11) et date MS-DOS.
            ZipArchiveEntry acc = z.GetEntry("dossier/é.txt");
            if (acc == null || Texte(acc) != "accent") return 6;
            DateTimeOffset d = z.Entries[0].LastWriteTime;
            if (d.Year != 2024 || d.Month != 5 || d.Day != 17 || d.Hour != 10 || d.Minute != 30) return 6;
            if (z.GetEntry("absente") != null) return 6;
        }

        // 7. Ce qui n'est pas une archive : InvalidDataException.
        try { new ZipArchive(new MemoryStream(new byte[100])); return 7; }
        catch (Exception e) { if (e.GetType().FullName != "System.IO.InvalidDataException") return 7; }

        // 8. Données Deflate corrompues (en-tête du bloc dynamique) :
        //    InvalidDataException à la lecture, jamais une autre exception.
        byte[] abime = (byte[])octets.Clone();
        abime[0x2F] = 0xFF; // bloc de type 3, réservé (RFC 1951 § 3.2.3)
        using (ZipArchive z = new ZipArchive(new MemoryStream(abime)))
        {
            try { Texte(z.Entries[0]); return 8; }
            catch (Exception e) { if (e.GetType().FullName != "System.IO.InvalidDataException") return 8; }
        }

        // 9. Une grosse archive fournie par scripts/test-zip.sh.
        string chemin = Environment.GetEnvironmentVariable("WINLUX_TEMOIN_ZIP");
        if (!string.IsNullOrEmpty(chemin))
        {
            using (ZipArchive z = new ZipArchive(File.OpenRead(chemin), ZipArchiveMode.Read))
            {
                if (z.Entries.Count == 0) return 9;
                foreach (ZipArchiveEntry e in z.Entries)
                {
                    byte[] attendus = File.ReadAllBytes(Path.Combine(chemin + ".d", e.FullName));
                    using (Stream s = e.Open())
                        if (!Egal(Lire(s), attendus)) return 9;
                }
            }
        }
        else Console.WriteLine("Compression : WINLUX_TEMOIN_ZIP absente, grosse archive sautée.");

        return 200;
    }
}
