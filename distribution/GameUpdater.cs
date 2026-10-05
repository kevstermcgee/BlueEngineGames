using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net;
using System.Security.Cryptography;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using System.Windows.Forms;

internal sealed class GameInfo
{
    public string Slug, Name, Description, Created, GameVersion, EngineVersion, Kind, Asset, Release, Sha256;
}

internal static class CatalogLogic
{
    public static List<GameInfo> ParseCatalog(string text)
    {
        var games = new List<GameInfo>();
        if (text == null) return games;
        foreach (var line in text.Replace("\r", "").Split('\n').Skip(1)) {
            var f = line.Split('\t');
            if (f.Length < 8 || !Regex.IsMatch(f[0], @"^[a-z0-9]+(?:-[a-z0-9]+)*$")) continue;
            games.Add(new GameInfo { Slug = f[0], Name = f[1], Description = f[2], Created = f[3],
                GameVersion = f[4], EngineVersion = f[5], Kind = f[6], Asset = f[7],
                Release = f.Length > 8 ? f[8] : "", Sha256 = f.Length > 9 ? f[9] : "" });
        }
        return games;
    }

    public static string TopLevelExe(string dir, string slug)
    {
        foreach (var name in new[] { slug + ".exe", "Play-" + slug + ".exe" }) {
            var path = Path.Combine(dir, name);
            if (File.Exists(path)) return path;
        }
        var exes = Directory.GetFiles(dir, "*.exe", SearchOption.TopDirectoryOnly)
            .Where(x => !Path.GetFileName(x).StartsWith("Update-", StringComparison.OrdinalIgnoreCase) &&
                !Path.GetFileName(x).StartsWith("unins", StringComparison.OrdinalIgnoreCase) &&
                !Path.GetFileName(x).Contains("server") && !Path.GetFileName(x).Contains("tools")).ToArray();
        return exes.Length == 1 ? exes[0] : null;
    }
}

// Package replacement is independent of the UI. The previous install stays intact until
// validation and preservation of player files have completed. A failed swap restores it.
internal static class InstallationLogic
{
    public const string Receipt = ".installed-release";
    public const string Manifest = ".installed-files.tsv";

    public static bool NeedsUpdate(GameInfo game, string dir)
    {
        if (String.IsNullOrEmpty(game.Asset)) return false;
        var receipt = Path.Combine(dir, Receipt);
        var identity = String.IsNullOrEmpty(game.Release) ? game.Asset : game.Release;
        return !File.Exists(receipt) || File.ReadAllText(receipt).Trim() != identity;
    }

    public static string FindExe(string dir, string slug)
    {
        if (!Directory.Exists(dir)) return null;
        var top = CatalogLogic.TopLevelExe(dir, slug);
        if (top != null) return top;
        return Directory.GetFiles(dir, "*.exe", SearchOption.AllDirectories).FirstOrDefault(x => {
            var n = Path.GetFileNameWithoutExtension(x).ToLowerInvariant();
            return !n.StartsWith("unins") && !n.Contains("server") && !n.Contains("tools") &&
                (n == "play-" + slug || n.Replace("-", "") == slug.Replace("-", ""));
        });
    }

    public static string Hash(string path)
    {
        using (var stream = File.OpenRead(path))
        using (var hash = SHA256.Create())
            return BitConverter.ToString(hash.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
    }

    public static void Install(GameInfo game, string zip, string dir, Action<string, string> moveDirectory = null)
    {
        if (moveDirectory == null) moveDirectory = Directory.Move;
        if (!String.IsNullOrEmpty(game.Sha256) &&
            !String.Equals(Hash(zip), game.Sha256, StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("The download failed its SHA-256 check. Your installed game was kept.");
        var parent = Directory.GetParent(dir).FullName;
        Directory.CreateDirectory(parent);
        // A separate lock survives renames and prevents two updater processes updating the same game.
        using (var installLock = new FileStream(Path.Combine(parent, "." + game.Slug + ".lock"),
            FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None)) {
            var staging = Path.Combine(parent, "." + game.Slug + ".installing-" + Guid.NewGuid().ToString("N"));
            var backup = Path.Combine(parent, "." + game.Slug + ".previous-" + Guid.NewGuid().ToString("N"));
            bool movedOld = false;
            try {
                ZipFile.ExtractToDirectory(zip, staging);
                if (FindExe(staging, game.Slug) == null)
                    throw new InvalidDataException("The downloaded package contains no playable .exe.");
                var shipped = Directory.GetFiles(staging, "*", SearchOption.AllDirectories)
                    .ToDictionary(x => x.Substring(staging.Length + 1), Hash, StringComparer.OrdinalIgnoreCase);
                if (shipped.ContainsKey(Receipt) || shipped.ContainsKey(Manifest))
                    throw new InvalidDataException("The package contains reserved installation metadata.");
                var oldFiles = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
                var oldManifest = Path.Combine(dir, Manifest);
                if (File.Exists(oldManifest)) {
                    foreach (var line in File.ReadAllLines(oldManifest)) {
                        var fields = line.Split('\t');
                        if (fields.Length == 2) oldFiles[fields[0]] = fields[1];
                    }
                }
                if (Directory.Exists(dir)) {
                    foreach (var file in Directory.GetFiles(dir, "*", SearchOption.AllDirectories)) {
                        var relative = file.Substring(dir.Length + 1);
                        if (relative == Receipt || relative == Manifest || shipped.ContainsKey(relative)) continue;
                        // Remove an obsolete package file only while it still has its shipped bytes.
                        if (oldFiles.ContainsKey(relative) && Hash(file) == oldFiles[relative]) continue;
                        var dest = Path.Combine(staging, relative);
                        Directory.CreateDirectory(Path.GetDirectoryName(dest));
                        File.Copy(file, dest, false);
                    }
                }
                File.WriteAllLines(Path.Combine(staging, Manifest), shipped.OrderBy(x => x.Key)
                    .Select(x => x.Key + "\t" + x.Value));
                File.WriteAllText(Path.Combine(staging, Receipt),
                    String.IsNullOrEmpty(game.Release) ? game.Asset : game.Release);
                if (Directory.Exists(dir)) { moveDirectory(dir, backup); movedOld = true; }
                try { moveDirectory(staging, dir); }
                catch { if (movedOld) moveDirectory(backup, dir); throw; }
                // A cleanup failure must not turn a completed installation into an error.
                if (movedOld) { try { Directory.Delete(backup, true); } catch (IOException) { } catch (UnauthorizedAccessException) { } }
            } finally {
                if (Directory.Exists(staging)) Directory.Delete(staging, true);
            }
        }
    }
}

// Each game gets this small helper, reachable from its Start Menu update shortcut.
// Run from a temporary copy so Windows can replace the installed helper too.
internal static class GameUpdater
{
    [STAThread]
    private static void Main(string[] args)
    {
        ServicePointManager.SecurityProtocol = (SecurityProtocolType)3072;
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        if (args.Length == 0) {
            foreach (var old in Directory.GetDirectories(Path.GetTempPath(), "BlueEngineUpdate-*")) {
                try { Directory.Delete(old, true); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            }
            var source = Application.ExecutablePath;
            var temp = Path.Combine(Path.GetTempPath(), "BlueEngineUpdate-" + Guid.NewGuid().ToString("N"));
            try {
                Directory.CreateDirectory(temp);
                var copy = Path.Combine(temp, Path.GetFileName(source));
                File.Copy(source, copy);
                Process.Start(new ProcessStartInfo(copy, "\"" + Path.GetDirectoryName(source) + "\"") { UseShellExecute = true });
            } catch (Exception ex) { MessageBox.Show(ex.Message, "Could not check for updates", MessageBoxButtons.OK, MessageBoxIcon.Error); }
            return;
        }
        var slug = Path.GetFileNameWithoutExtension(Application.ExecutablePath).Substring("Update-".Length);
        var dir = Path.GetFullPath(args[0]);
        var form = new Form { Text = "Game update", StartPosition = FormStartPosition.CenterScreen,
            ClientSize = new Size(460, 100), FormBorderStyle = FormBorderStyle.FixedDialog, MaximizeBox = false };
        var status = new Label { Text = "Checking for updates…", Dock = DockStyle.Fill, Padding = new Padding(16), AutoSize = false };
        form.Controls.Add(status);
        bool committing = false;
        form.FormClosing += delegate(object sender, FormClosingEventArgs e) { if (committing) e.Cancel = true; };
        form.Shown += async delegate {
            string zip = null;
            try {
                GameInfo game;
                using (var client = new WebClient()) {
                    client.Headers.Add("User-Agent", "BlueEngine-Game-Updater");
                    var text = await client.DownloadStringTaskAsync(new Uri("https://github.com/kevstermcgee/BlueEngineGames/releases/latest/download/Games-catalog.tsv"));
                    game = CatalogLogic.ParseCatalog(text).FirstOrDefault(x => x.Slug == slug);
                }
                if (game == null) throw new InvalidDataException("This game is not available in the latest release. You can still play your installed copy.");
                if (!InstallationLogic.NeedsUpdate(game, dir)) {
                    MessageBox.Show(form, game.Name + " is up to date.", "Game update"); return;
                }
                if (String.IsNullOrEmpty(game.Release) || !Regex.IsMatch(game.Sha256 ?? "", @"^[a-fA-F0-9]{64}$"))
                    throw new InvalidDataException("The release is missing version or checksum information.");
                if (MessageBox.Show(form, "Update " + game.Name + " to release " + game.Release + "?\n\nClose the game first. Your saves and settings will be kept.",
                    "Game update", MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes) return;
                status.Text = "Downloading " + game.Name + "…";
                zip = Path.Combine(Path.GetTempPath(), "BlueEngineUpdate-" + Guid.NewGuid().ToString("N") + ".zip");
                using (var client = new WebClient()) {
                    client.Headers.Add("User-Agent", "BlueEngine-Game-Updater");
                    await client.DownloadFileTaskAsync(new Uri(game.Asset), zip);
                }
                status.Text = "Installing update…";
                committing = true;
                try { await Task.Run(() => InstallationLogic.Install(game, zip, dir)); }
                finally { committing = false; }
                MessageBox.Show(form, game.Name + " has been updated.", "Game update");
            } catch (Exception ex) {
                MessageBox.Show(form, ex.Message + "\n\nIf the game is running, close it and try again.", "Could not update game", MessageBoxButtons.OK, MessageBoxIcon.Error);
            } finally {
                if (zip != null && File.Exists(zip)) { try { File.Delete(zip); } catch (IOException) { } }
                form.Close();
            }
        };
        Application.Run(form);
    }
}
