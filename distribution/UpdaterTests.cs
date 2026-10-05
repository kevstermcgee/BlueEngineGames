using System;
using System.IO;
using System.IO.Compression;
using System.Linq;

internal static class UpdaterTests
{
    private static void Check(bool condition, string message) { if (!condition) throw new Exception(message); }
    private static void Reject(Action action, string message) {
        bool rejected = false;
        try { action(); } catch (InvalidDataException) { rejected = true; } catch (IOException) { rejected = true; } catch (UnauthorizedAccessException) { rejected = true; }
        Check(rejected, message);
    }
    private static string Package(string root, string name, params string[] files) {
        var stage = Path.Combine(root, name); Directory.CreateDirectory(stage);
        foreach (var file in files) File.WriteAllText(Path.Combine(stage, file), name + ":" + file);
        var zip = stage + ".zip"; ZipFile.CreateFromDirectory(stage, zip); return zip;
    }
    private static void Main() {
        var root = Path.Combine(Path.GetTempPath(), "BlueEngineUpdaterTests-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try {
            var dir = Path.Combine(root, "games", "test-game"); Directory.CreateDirectory(dir);
            var game = new GameInfo { Slug = "test-game", Name = "Test", Release = "release-one", Asset = "https://example.com/one.zip" };
            File.WriteAllText(Path.Combine(dir, "Play-test-game.exe"), "legacy");
            Directory.CreateDirectory(Path.Combine(dir, "saves"));
            File.WriteAllText(Path.Combine(dir, "saves", "quick.be2save"), "player progress");
            File.WriteAllText(Path.Combine(dir, "audio-settings.json"), "player settings");
            Check(InstallationLogic.NeedsUpdate(game, dir), "Legacy install should be updateable");
            var one = Package(root, "one", "Play-test-game.exe", "obsolete.dat", "modified.dat");
            game.Sha256 = InstallationLogic.Hash(one);
            InstallationLogic.Install(game, one, dir);
            Check(!InstallationLogic.NeedsUpdate(game, dir), "Receipt must record the installed release");
            Check(File.ReadAllText(Path.Combine(dir, "saves", "quick.be2save")) == "player progress", "Legacy saves lost");
            Check(File.ReadAllText(Path.Combine(dir, "audio-settings.json")) == "player settings", "Legacy settings lost");
            File.WriteAllText(Path.Combine(dir, "modified.dat"), "player edited");
            var two = Package(root, "two", "Play-test-game.exe", "new.dat");
            game.Release = "release-two"; game.Sha256 = InstallationLogic.Hash(two);
            Check(InstallationLogic.NeedsUpdate(game, dir), "Same semantic version, new release must be detected");
            InstallationLogic.Install(game, two, dir);
            Check(!File.Exists(Path.Combine(dir, "obsolete.dat")), "Obsolete shipped file must be removed");
            Check(File.ReadAllText(Path.Combine(dir, "modified.dat")) == "player edited", "Modified obsolete file must be preserved");
            Check(File.Exists(Path.Combine(dir, "new.dat")), "Updated payload missing");
            Check(!InstallationLogic.NeedsUpdate(game, dir), "Successful update still offered");
            var before = File.ReadAllText(Path.Combine(dir, "Play-test-game.exe"));
            game.Release = "release-three"; game.Sha256 = new string('0', 64);
            Reject(() => InstallationLogic.Install(game, one, dir), "Bad checksum accepted");
            Check(File.ReadAllText(Path.Combine(dir, "Play-test-game.exe")) == before, "Bad checksum destroyed installed executable");
            var bad = Package(root, "bad", "no-game.txt"); game.Sha256 = InstallationLogic.Hash(bad);
            Reject(() => InstallationLogic.Install(game, bad, dir), "Non-playable package accepted");
            Check(File.ReadAllText(Path.Combine(dir, InstallationLogic.Receipt)) == "release-two", "Invalid package changed receipt");
            game.Sha256 = InstallationLogic.Hash(one);
            Reject(() => InstallationLogic.Install(game, one, dir, (from, to) => {
                if (from.Contains(".installing-")) throw new IOException("Simulated failed commit");
                Directory.Move(from, to);
            }), "Failed commit did not report failure");
            Check(File.ReadAllText(Path.Combine(dir, "Play-test-game.exe")) == before, "Failed commit did not restore previous install");
            Check(File.ReadAllText(Path.Combine(dir, "saves", "quick.be2save")) == "player progress", "Failed commit lost saves");
            Check(!Directory.GetDirectories(Path.GetDirectoryName(dir)).Any(x => x.Contains(".installing-")), "Staging files leaked");
            var escape = Path.Combine(root, "escape.zip");
            using (var zip = ZipFile.Open(escape, ZipArchiveMode.Create)) {
                using (var writer = new StreamWriter(zip.CreateEntry("../escape.txt").Open())) writer.Write("escape");
            }
            game.Sha256 = InstallationLogic.Hash(escape);
            Reject(() => InstallationLogic.Install(game, escape, dir), "Archive traversal accepted");
            Check(!File.Exists(Path.Combine(root, "games", "escape.txt")), "Archive escaped staging");
            var catalog = "header\n../bad\tBad\tx\tx\tx\tx\tx\tx\n" +
                "test-game\tTest\tDescription\t2026-01-01\t0.1.0\tengine\tnative\turl\trelease-two\t" + new string('a',64);
            var parsed = CatalogLogic.ParseCatalog(catalog);
            Check(parsed.Count == 1 && parsed[0].Release == "release-two", "Catalog version parsing or slug validation failed");
            Console.WriteLine("Updater tests passed: legacy migration, version detection, player files, pruning, checksum failure, invalid ZIP, rollback, traversal.");
        } finally { Directory.Delete(root, true); }
    }
}
