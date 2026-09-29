using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using System.Windows.Forms;

internal sealed class GameInfo
{
    public string Slug, Name, Description, Created, GameVersion, EngineVersion, Kind, Asset;
}

// Catalog parsing, merge and local-game discovery. Kept static so it can be tested without a display.
internal static class CatalogLogic
{
    public static List<GameInfo> ParseCatalog(string text)
    {
        var games = new List<GameInfo>();
        if (text == null) return games;
        foreach (var line in text.Replace("\r", "").Split('\n').Skip(1)) {
            var f = line.Split('\t');
            if (f.Length != 8 || f[0].Length == 0) continue;
            games.Add(new GameInfo { Slug = f[0], Name = f[1], Description = f[2], Created = f[3],
                GameVersion = f[4], EngineVersion = f[5], Kind = f[6], Asset = f[7] });
        }
        return games;
    }

    // Merge by slug: the shipped/local catalog is the base, remote rows replace matching slugs,
    // and games that exist only locally (catalog rows or discovered folders) are always kept.
    public static List<GameInfo> Merge(IEnumerable<GameInfo> local, IEnumerable<GameInfo> remote, IEnumerable<GameInfo> discovered)
    {
        var order = new List<string>();
        var bySlug = new Dictionary<string, GameInfo>(StringComparer.OrdinalIgnoreCase);
        foreach (var game in (local ?? new GameInfo[0]).Concat(remote ?? new GameInfo[0])) {
            if (!bySlug.ContainsKey(game.Slug)) order.Add(game.Slug);
            bySlug[game.Slug] = game;
        }
        foreach (var game in discovered ?? new GameInfo[0]) {
            if (bySlug.ContainsKey(game.Slug)) continue;
            order.Add(game.Slug);
            bySlug[game.Slug] = game;
        }
        return order.Select(x => bySlug[x]).ToList();
    }

    // <slug>.exe, then Play-<slug>.exe, then the only .exe in the folder.
    public static string TopLevelExe(string dir, string slug)
    {
        foreach (var name in new[] { slug + ".exe", "Play-" + slug + ".exe" }) {
            var path = Path.Combine(dir, name);
            if (File.Exists(path)) return path;
        }
        var exes = Directory.GetFiles(dir, "*.exe", SearchOption.TopDirectoryOnly);
        return exes.Length == 1 ? exes[0] : null;
    }

    public static List<GameInfo> DiscoverLocal(string gamesRoot)
    {
        var found = new List<GameInfo>();
        if (!Directory.Exists(gamesRoot)) return found;
        foreach (var dir in Directory.GetDirectories(gamesRoot)) {
            try {
                var slug = Path.GetFileName(dir);
                if (slug.StartsWith(".") || slug.EndsWith(".installing", StringComparison.OrdinalIgnoreCase)) continue;
                if (TopLevelExe(dir, slug) == null) continue;
                var meta = ReadLocalMeta(dir);
                Func<string, string, string> get = (key, fallback) => meta.ContainsKey(key) ? meta[key] : fallback;
                var title = CultureInfo.InvariantCulture.TextInfo.ToTitleCase(slug.Replace('-', ' ').Replace('_', ' '));
                found.Add(new GameInfo { Slug = slug, Name = get("name", title),
                    Description = get("description", "Local game found in your games folder."),
                    Created = get("created", Directory.GetCreationTime(dir).ToString("yyyy-MM-dd")),
                    GameVersion = get("version", "local"), EngineVersion = get("engine", "local"),
                    Kind = "local game", Asset = "" });
            } catch (Exception) { }
        }
        return found;
    }

    // Optional metadata: game.json, manifest.tsv (key<TAB>value lines), then ship.json. First value wins.
    private static Dictionary<string, string> ReadLocalMeta(string dir)
    {
        var meta = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        Action<string, string> set = (key, value) => {
            value = Regex.Replace(value ?? "", @"\s+", " ").Trim();
            if (value.Length > 0 && !meta.ContainsKey(key)) meta[key] = value;
        };
        set("name", JsonString(dir, "game.json", "name"));
        set("name", JsonString(dir, "game.json", "title"));
        foreach (var key in new[] { "description", "version", "created" })
            set(key, JsonString(dir, "game.json", key));
        var manifest = Path.Combine(dir, "manifest.tsv");
        if (File.Exists(manifest))
            foreach (var line in File.ReadAllLines(manifest)) {
                var f = line.Split(new[] { '\t' }, 2);
                if (f.Length == 2) set(f[0].Trim() == "title" ? "name" : f[0].Trim(), f[1]);
            }
        set("name", JsonString(dir, "ship.json", "title"));
        var packaged = JsonString(dir, "ship.json", "packaged_at");
        set("created", packaged != null && packaged.Length >= 10 ? packaged.Substring(0, 10) : null);
        set("engine", JsonString(dir, "ship.json", "engine_revision"));
        return meta;
    }

    private static string JsonString(string dir, string file, string key)
    {
        var path = Path.Combine(dir, file);
        if (!File.Exists(path)) return null;
        var m = Regex.Match(File.ReadAllText(path), "\"" + Regex.Escape(key) + "\"\\s*:\\s*\"((?:[^\"\\\\]|\\\\.)*)\"");
        return m.Success ? Regex.Unescape(m.Groups[1].Value) : null;
    }
}

internal sealed class LauncherForm : Form
{
    private readonly string baseDir = AppDomain.CurrentDomain.BaseDirectory;
    private readonly string product, repository, installFolder, catalogUrl;
    private readonly Color accent, background, surface, cardColor, muted;
    private readonly Panel viewport = new Panel(), scrollTrack = new Panel(), scrollThumb = new Panel();
    private readonly FlowLayoutPanel gamesPanel = new FlowLayoutPanel();
    private readonly Label status = new Label();
    private int scrollOffset, dragStartY, dragStartTop;
    private bool draggingThumb;

    public LauncherForm()
    {
        var settings = ReadSettings(Path.Combine(baseDir, "launcher-settings.tsv"));
        product = settings["product"];
        repository = settings["repository"];
        installFolder = settings["install_folder"];
        catalogUrl = settings["catalog_url"];
        accent = ReadColor(settings, "accent", "#2F81F7");
        background = ReadColor(settings, "background", "#090F1D");
        surface = ReadColor(settings, "surface", "#111C31");
        cardColor = ReadColor(settings, "card", "#172641");
        muted = ReadColor(settings, "muted", "#91A6C6");

        Text = product;
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(720, 500);
        MinimumSize = new Size(600, 390);
        BackColor = background;
        ForeColor = Color.White;
        Font = new Font("Segoe UI", 10F);
        Icon = SystemIcons.Application;

        var header = new Panel { Dock = DockStyle.Top, Height = 72, BackColor = surface };
        var title = new Label { Text = product, AutoSize = true, ForeColor = Color.White,
            Font = new Font("Segoe UI Semibold", 19F), Location = new Point(20, 18) };
        var downloads = MakeQuietButton("Downloads", 94);
        downloads.Location = new Point(606, 18);
        downloads.Anchor = AnchorStyles.Top | AnchorStyles.Right;
        downloads.Click += delegate { OpenUrl("https://github.com/" + repository + "/releases/latest"); };
        header.Controls.AddRange(new Control[] { title, downloads });

        var footer = new Panel { Dock = DockStyle.Bottom, Height = 32, BackColor = surface };
        status.Text = "Ready";
        status.AutoEllipsis = true;
        status.ForeColor = muted;
        status.Location = new Point(20, 8);
        status.Size = new Size(675, 18);
        status.Anchor = AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Top;
        footer.Controls.Add(status);

        viewport.Dock = DockStyle.Fill;
        viewport.BackColor = background;
        viewport.Resize += delegate { LayoutGameList(); };
        viewport.MouseWheel += OnListMouseWheel;

        gamesPanel.FlowDirection = FlowDirection.TopDown;
        gamesPanel.WrapContents = false;
        gamesPanel.AutoSize = true;
        gamesPanel.AutoSizeMode = AutoSizeMode.GrowAndShrink;
        gamesPanel.BackColor = background;
        gamesPanel.Location = new Point(16, 12);
        gamesPanel.MouseWheel += OnListMouseWheel;
        viewport.Controls.Add(gamesPanel);

        scrollTrack.Width = 8;
        scrollTrack.BackColor = surface;
        scrollTrack.Cursor = Cursors.Hand;
        scrollTrack.MouseDown += delegate(object sender, MouseEventArgs e) {
            if (!scrollThumb.Bounds.Contains(e.Location)) SetScrollFromThumb(e.Y - scrollThumb.Height / 2);
        };
        scrollThumb.BackColor = accent;
        scrollThumb.Cursor = Cursors.Hand;
        scrollThumb.MouseDown += delegate {
            draggingThumb = true; dragStartY = Cursor.Position.Y; dragStartTop = scrollThumb.Top; scrollThumb.Capture = true;
        };
        scrollThumb.MouseMove += delegate {
            if (draggingThumb) SetScrollFromThumb(dragStartTop + Cursor.Position.Y - dragStartY);
        };
        scrollThumb.MouseUp += delegate { draggingThumb = false; scrollThumb.Capture = false; };
        scrollTrack.Controls.Add(scrollThumb);
        viewport.Controls.Add(scrollTrack);

        Controls.Add(viewport);
        Controls.Add(footer);
        Controls.Add(header);
        MouseWheel += OnListMouseWheel;
        LoadGames();
        Shown += async delegate { await RefreshCatalog(); };
    }

    private static Color ReadColor(Dictionary<string, string> settings, string key, string fallback)
    {
        return ColorTranslator.FromHtml(settings.ContainsKey(key) ? settings[key] : fallback);
    }

    private Dictionary<string, string> ReadSettings(string path)
    {
        if (!File.Exists(path)) throw new FileNotFoundException("Launcher settings are missing.", path);
        return File.ReadAllLines(path).Where(x => x.Contains("\t"))
            .Select(x => x.Split(new[] { '\t' }, 2))
            .ToDictionary(x => x[0], x => x[1], StringComparer.OrdinalIgnoreCase);
    }

    private string LauncherDataDirectory()
    {
        return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), installFolder);
    }

    private string RemoteCatalogCache()
    {
        return Path.Combine(LauncherDataDirectory(), "remote-catalog.tsv");
    }

    // Shipped catalog + cached remote catalog (remote wins) + games discovered in the install folder.
    private void LoadGames()
    {
        gamesPanel.Controls.Clear();
        var path = Path.Combine(baseDir, "launcher-catalog.tsv");
        var local = File.Exists(path) ? CatalogLogic.ParseCatalog(File.ReadAllText(path)) : new List<GameInfo>();
        var cache = RemoteCatalogCache();
        var remote = File.Exists(cache) ? CatalogLogic.ParseCatalog(File.ReadAllText(cache)) : new List<GameInfo>();
        var discovered = CatalogLogic.DiscoverLocal(Path.Combine(LauncherDataDirectory(), "games"));
        var games = CatalogLogic.Merge(local, remote, discovered);
        if (games.Count == 0 && !File.Exists(path)) { ShowError("The launcher catalog is missing: " + path); return; }
        foreach (var game in games.OrderBy(x => x.Name)) gamesPanel.Controls.Add(MakeRow(game));
        scrollOffset = 0;
        LayoutGameList();
        status.Text = games.Count + " games";
    }

    // The remote catalog is only cached, never written over the shipped catalog, so local games are not lost.
    private async Task RefreshCatalog()
    {
        try {
            status.Text = "Checking for new games…";
            string text;
            using (var client = new WebClient()) {
                client.Headers.Add("User-Agent", product.Replace(" ", "-"));
                text = await client.DownloadStringTaskAsync(new Uri(catalogUrl));
            }
            if (CatalogLogic.ParseCatalog(text).Count == 0) throw new InvalidDataException("Remote catalog has no games.");
            var cache = RemoteCatalogCache();
            if (!File.Exists(cache) || !String.Equals(File.ReadAllText(cache), text, StringComparison.Ordinal)) {
                Directory.CreateDirectory(LauncherDataDirectory());
                File.WriteAllText(cache, text); LoadGames();
            } else status.Text = gamesPanel.Controls.Count + " games";
        } catch { status.Text = gamesPanel.Controls.Count + " games · offline"; }
    }

    private Control MakeRow(GameInfo game)
    {
        var row = new Panel { Height = 66, Width = 650, BackColor = cardColor, Margin = new Padding(0, 0, 0, 8) };
        var marker = new Panel { BackColor = accent, Location = new Point(0, 0), Size = new Size(4, 66) };
        var name = new Label { Text = game.Name, AutoEllipsis = true, ForeColor = Color.White,
            Font = new Font("Segoe UI Semibold", 12F), Location = new Point(17, 11), Size = new Size(380, 24),
            Anchor = AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Top };
        var hint = new Label { Text = FindGameExe(game) != null ? "Installed" :
            String.IsNullOrEmpty(game.Asset) ? "Not found locally" : "Ready to install", AutoSize = true,
            ForeColor = muted, Font = new Font("Segoe UI", 8.5F), Location = new Point(18, 38) };
        var info = MakeQuietButton("i", 34);
        info.Location = new Point(row.Width - 148, 15);
        info.Anchor = AnchorStyles.Top | AnchorStyles.Right;
        info.Click += delegate { ShowGameDetails(game); };
        var action = MakeButton(FindGameExe(game) == null ? "Install" : "Play", 96);
        action.Location = new Point(row.Width - 106, 15);
        action.Anchor = AnchorStyles.Top | AnchorStyles.Right;
        action.Click += async delegate {
            action.Enabled = false;
            try { await InstallAndPlay(game); action.Text = "Play"; hint.Text = "Installed"; }
            finally { action.Enabled = true; }
        };
        row.Controls.AddRange(new Control[] { marker, name, hint, info, action });
        WireWheel(row);
        return row;
    }

    private void ShowGameDetails(GameInfo game)
    {
        MessageBox.Show(this, game.Description + Environment.NewLine + Environment.NewLine +
            "Created: " + game.Created + Environment.NewLine +
            "Game version: " + game.GameVersion + Environment.NewLine +
            "Engine: " + game.EngineVersion + Environment.NewLine +
            "Package: " + game.Kind, game.Name, MessageBoxButtons.OK, MessageBoxIcon.Information);
    }

    private void WireWheel(Control control)
    {
        control.MouseWheel += OnListMouseWheel;
        foreach (Control child in control.Controls) child.MouseWheel += OnListMouseWheel;
    }

    private void OnListMouseWheel(object sender, MouseEventArgs e)
    {
        ScrollTo(scrollOffset + (e.Delta > 0 ? -72 : 72));
    }

    private void LayoutGameList()
    {
        int width = Math.Max(480, viewport.ClientSize.Width - 42);
        foreach (Control row in gamesPanel.Controls) row.Width = width;
        gamesPanel.Width = width;
        int contentHeight = gamesPanel.Controls.Cast<Control>().Sum(x => x.Height + x.Margin.Vertical);
        gamesPanel.Height = Math.Max(1, contentHeight);
        scrollTrack.Location = new Point(Math.Max(0, viewport.ClientSize.Width - 14), 12);
        scrollTrack.Height = Math.Max(1, viewport.ClientSize.Height - 24);
        int visible = Math.Max(1, viewport.ClientSize.Height - 24);
        scrollThumb.Size = new Size(8, contentHeight <= visible ? scrollTrack.Height :
            Math.Max(34, scrollTrack.Height * visible / contentHeight));
        scrollTrack.Visible = contentHeight > visible;
        scrollTrack.BringToFront();
        ScrollTo(scrollOffset);
    }

    private void ScrollTo(int value)
    {
        int visible = Math.Max(1, viewport.ClientSize.Height - 24);
        int max = Math.Max(0, gamesPanel.Height - visible);
        scrollOffset = Math.Max(0, Math.Min(max, value));
        gamesPanel.Top = 12 - scrollOffset;
        scrollThumb.Top = max > 0 && scrollTrack.Height > scrollThumb.Height
            ? scrollOffset * (scrollTrack.Height - scrollThumb.Height) / max : 0;
    }

    private void SetScrollFromThumb(int top)
    {
        int travel = Math.Max(1, scrollTrack.Height - scrollThumb.Height);
        top = Math.Max(0, Math.Min(travel, top));
        int max = Math.Max(0, gamesPanel.Height - Math.Max(1, viewport.ClientSize.Height - 24));
        ScrollTo(top * max / travel);
    }

    private Button MakeButton(string text, int width)
    {
        var button = new Button { Text = text, Width = width, Height = 36, FlatStyle = FlatStyle.Flat,
            BackColor = accent, ForeColor = Color.White, Cursor = Cursors.Hand,
            Font = new Font("Segoe UI Semibold", 9F), UseVisualStyleBackColor = false };
        button.FlatAppearance.BorderSize = 0;
        return button;
    }

    private Button MakeQuietButton(string text, int width)
    {
        var button = MakeButton(text, width);
        button.BackColor = cardColor;
        button.FlatAppearance.BorderSize = 1;
        button.FlatAppearance.BorderColor = accent;
        return button;
    }

    private string GameDirectory(GameInfo game)
    {
        return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            installFolder, "games", game.Slug);
    }

    private string FindGameExe(GameInfo game)
    {
        var dir = GameDirectory(game);
        if (!Directory.Exists(dir)) return null;
        var exact = Directory.GetFiles(dir, "Play-" + game.Slug + ".exe", SearchOption.AllDirectories).FirstOrDefault();
        if (exact != null) return exact;
        var topLevel = CatalogLogic.TopLevelExe(dir, game.Slug);
        if (topLevel != null) return topLevel;
        var normalizedSlug = game.Slug.Replace("-", "");
        var candidates = Directory.GetFiles(dir, "*.exe", SearchOption.AllDirectories).Where(x => {
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
                if (String.IsNullOrEmpty(game.Asset))
                    throw new InvalidOperationException(game.Name + " has no download and no .exe was found in " + GameDirectory(game) + ".");
                status.Text = "Installing " + game.Name + "…";
                var dir = GameDirectory(game);
                Directory.CreateDirectory(Directory.GetParent(dir).FullName);
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
        } catch (Exception ex) { status.Text = "Could not launch " + game.Name; ShowError(ex.Message); }
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
