using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net;
using System.Threading.Tasks;
using System.Windows.Forms;

internal sealed class GameInfo
{
    public string Slug, Name, Description, Created, GameVersion, EngineVersion, Kind, Asset;
}

internal sealed class LauncherForm : Form
{
    private readonly string baseDir = AppDomain.CurrentDomain.BaseDirectory;
    private readonly string product;
    private readonly string repository;
    private readonly string installFolder;
    private readonly string catalogUrl;
    private readonly Color accent;
    private readonly FlowLayoutPanel gamesPanel = new FlowLayoutPanel();
    private readonly Label status = new Label();

    public LauncherForm()
    {
        var settings = ReadSettings(Path.Combine(baseDir, "launcher-settings.tsv"));
        product = settings["product"];
        repository = settings["repository"];
        installFolder = settings["install_folder"];
        catalogUrl = settings["catalog_url"];
        accent = ColorTranslator.FromHtml(settings["accent"]);

        Text = product;
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(980, 680);
        MinimumSize = new Size(760, 520);
        BackColor = Color.FromArgb(11, 16, 28);
        ForeColor = Color.White;
        Font = new Font("Segoe UI", 10F);
        Icon = SystemIcons.Application;

        var header = new Panel { Dock = DockStyle.Top, Height = 118, BackColor = Color.FromArgb(16, 24, 40) };
        var eyebrow = new Label {
            Text = "ENGINE GAMES  /  WINDOWS", AutoSize = true, ForeColor = accent,
            Font = new Font("Segoe UI Semibold", 9F), Location = new Point(28, 19)
        };
        var title = new Label {
            Text = product, AutoSize = true, ForeColor = Color.White,
            Font = new Font("Segoe UI Semibold", 25F), Location = new Point(24, 39)
        };
        var subtitle = new Label {
            Text = "Install once. Launch fast. Every game stays in its own lane.",
            AutoSize = true, ForeColor = Color.FromArgb(155, 168, 190), Location = new Point(29, 82)
        };
        var downloads = MakeButton("Release downloads", 150);
        downloads.Location = new Point(790, 39);
        downloads.Anchor = AnchorStyles.Top | AnchorStyles.Right;
        downloads.Click += delegate { OpenUrl("https://github.com/" + repository + "/releases/latest"); };
        header.Controls.AddRange(new Control[] { eyebrow, title, subtitle, downloads });

        gamesPanel.Dock = DockStyle.Fill;
        gamesPanel.AutoScroll = true;
        gamesPanel.FlowDirection = FlowDirection.TopDown;
        gamesPanel.WrapContents = false;
        gamesPanel.Padding = new Padding(24, 20, 24, 20);
        gamesPanel.BackColor = BackColor;
        gamesPanel.SizeChanged += delegate {
            foreach (Control c in gamesPanel.Controls) c.Width = Math.Max(620, gamesPanel.ClientSize.Width - 55);
        };

        var footer = new Panel { Dock = DockStyle.Bottom, Height = 42, BackColor = Color.FromArgb(16, 24, 40) };
        status.Text = "Ready";
        status.AutoEllipsis = true;
        status.ForeColor = Color.FromArgb(155, 168, 190);
        status.Location = new Point(28, 12);
        status.Size = new Size(900, 22);
        status.Anchor = AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Top;
        footer.Controls.Add(status);

        Controls.Add(gamesPanel);
        Controls.Add(footer);
        Controls.Add(header);
        LoadGames();
        Shown += async delegate { await RefreshCatalog(); };
    }

    private Dictionary<string, string> ReadSettings(string path)
    {
        if (!File.Exists(path)) throw new FileNotFoundException("Launcher settings are missing.", path);
        return File.ReadAllLines(path).Where(x => x.Contains("\t"))
            .Select(x => x.Split(new[] { '\t' }, 2))
            .ToDictionary(x => x[0], x => x[1], StringComparer.OrdinalIgnoreCase);
    }

    private void LoadGames()
    {
        gamesPanel.Controls.Clear();
        var path = Path.Combine(baseDir, "launcher-catalog.tsv");
        if (!File.Exists(path)) {
            ShowError("The launcher catalog is missing: " + path);
            return;
        }
        var games = new List<GameInfo>();
        foreach (var line in File.ReadAllLines(path).Skip(1)) {
            var f = line.Split('\t');
            if (f.Length != 8) continue;
            games.Add(new GameInfo {
                Slug = f[0], Name = f[1], Description = f[2], Created = f[3],
                GameVersion = f[4], EngineVersion = f[5], Kind = f[6], Asset = f[7]
            });
        }
        foreach (var game in games.OrderBy(x => x.Name)) gamesPanel.Controls.Add(MakeCard(game));
        status.Text = games.Count + " games available · Downloads are verified against the latest GitHub release";
    }

    private async Task RefreshCatalog()
    {
        try {
            status.Text = "Checking for new games…";
            string text;
            using (var client = new WebClient()) {
                client.Headers.Add("User-Agent", product.Replace(" ", "-"));
                text = await client.DownloadStringTaskAsync(new Uri(catalogUrl));
            }
            var path = Path.Combine(baseDir, "launcher-catalog.tsv");
            if (!String.Equals(File.ReadAllText(path), text, StringComparison.Ordinal)) {
                File.WriteAllText(path, text);
                LoadGames();
            } else {
                status.Text = gamesPanel.Controls.Count + " games available · Catalog is current";
            }
        } catch {
            status.Text = gamesPanel.Controls.Count + " games available · Offline catalog";
        }
    }

    private Control MakeCard(GameInfo game)
    {
        var card = new Panel {
            Height = 126, Width = Math.Max(620, gamesPanel.ClientSize.Width - 55),
            BackColor = Color.FromArgb(21, 30, 49), Margin = new Padding(0, 0, 0, 12)
        };
        var marker = new Panel { BackColor = accent, Location = new Point(0, 0), Size = new Size(5, 126) };
        var name = new Label {
            Text = game.Name, AutoSize = true, ForeColor = Color.White,
            Font = new Font("Segoe UI Semibold", 15F), Location = new Point(22, 15)
        };
        var desc = new Label {
            Text = game.Description, AutoEllipsis = true, ForeColor = Color.FromArgb(184, 195, 214),
            Location = new Point(25, 49), Size = new Size(card.Width - 210, 24), Anchor = AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Top
        };
        var meta = new Label {
            Text = "Created " + game.Created + "   ·   Game " + game.GameVersion +
                   "   ·   Engine " + game.EngineVersion + "   ·   " + game.Kind,
            AutoEllipsis = true, ForeColor = Color.FromArgb(116, 137, 169),
            Font = new Font("Segoe UI", 9F), Location = new Point(25, 84),
            Size = new Size(card.Width - 210, 22), Anchor = AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Top
        };
        var action = MakeButton(FindGameExe(game) == null ? "Install & play" : "Play", 142);
        action.Location = new Point(card.Width - 166, 41);
        action.Anchor = AnchorStyles.Top | AnchorStyles.Right;
        action.Click += async delegate {
            action.Enabled = false;
            try {
                await InstallAndPlay(game);
                action.Text = "Play";
            } finally {
                action.Enabled = true;
            }
        };
        card.Controls.AddRange(new Control[] { marker, name, desc, meta, action });
        return card;
    }

    private Button MakeButton(string text, int width)
    {
        var button = new Button {
            Text = text, Width = width, Height = 38, FlatStyle = FlatStyle.Flat,
            BackColor = accent, ForeColor = Color.White, Cursor = Cursors.Hand,
            Font = new Font("Segoe UI Semibold", 9.5F), UseVisualStyleBackColor = false
        };
        button.FlatAppearance.BorderSize = 0;
        return button;
    }

    private string GameDirectory(GameInfo game)
    {
        var root = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), installFolder, "games");
        return Path.Combine(root, game.Slug);
    }

    private string FindGameExe(GameInfo game)
    {
        var dir = GameDirectory(game);
        if (!Directory.Exists(dir)) return null;
        var exact = Directory.GetFiles(dir, "Play-" + game.Slug + ".exe", SearchOption.AllDirectories).FirstOrDefault();
        if (exact != null) return exact;
        var normalizedSlug = game.Slug.Replace("-", "");
        var candidates = Directory.GetFiles(dir, "*.exe", SearchOption.AllDirectories)
            .Where(x => {
                var n = Path.GetFileNameWithoutExtension(x).ToLowerInvariant();
                return !n.Contains("server") && !n.Contains("tools") &&
                       (n.Replace("-", "") == normalizedSlug || !n.Contains("engine"));
            }).ToArray();
        return candidates.FirstOrDefault() ?? Directory.GetFiles(dir, "*.exe", SearchOption.AllDirectories)
            .FirstOrDefault(x => !Path.GetFileNameWithoutExtension(x).ToLowerInvariant().Contains("server"));
    }

    private async Task InstallAndPlay(GameInfo game)
    {
        try {
            var exe = FindGameExe(game);
            if (exe == null) {
                status.Text = "Downloading " + game.Name + "…";
                var dir = GameDirectory(game);
                var parent = Directory.GetParent(dir).FullName;
                Directory.CreateDirectory(parent);
                var zip = Path.Combine(Path.GetTempPath(), game.Slug + "-" + Guid.NewGuid().ToString("N") + ".zip");
                var staging = dir + ".installing";
                if (Directory.Exists(staging)) Directory.Delete(staging, true);
                Directory.CreateDirectory(staging);
                using (var client = new WebClient()) {
                    client.Headers.Add("User-Agent", product.Replace(" ", "-"));
                    await client.DownloadFileTaskAsync(new Uri(game.Asset), zip);
                }
                ZipFile.ExtractToDirectory(zip, staging);
                File.Delete(zip);
                if (Directory.Exists(dir)) Directory.Delete(dir, true);
                Directory.Move(staging, dir);
                exe = FindGameExe(game);
                if (exe == null) throw new InvalidOperationException("The downloaded package contains no playable .exe.");
            }
            status.Text = "Launching " + game.Name + "…";
            Process.Start(new ProcessStartInfo(exe) { WorkingDirectory = Path.GetDirectoryName(exe), UseShellExecute = true });
            status.Text = game.Name + " is running";
        } catch (Exception ex) {
            status.Text = "Could not launch " + game.Name;
            ShowError(ex.Message);
        }
    }

    private static void OpenUrl(string url)
    {
        try { Process.Start(new ProcessStartInfo(url) { UseShellExecute = true }); }
        catch (Exception ex) { MessageBox.Show(ex.Message, "Could not open link", MessageBoxButtons.OK, MessageBoxIcon.Error); }
    }

    private void ShowError(string message)
    {
        MessageBox.Show(this, message, product, MessageBoxButtons.OK, MessageBoxIcon.Error);
    }

    [STAThread]
    private static void Main()
    {
        ServicePointManager.SecurityProtocol = (SecurityProtocolType)3072;
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        try { Application.Run(new LauncherForm()); }
        catch (Exception ex) { MessageBox.Show(ex.ToString(), "Engine Games Launcher", MessageBoxButtons.OK, MessageBoxIcon.Error); }
    }
}
