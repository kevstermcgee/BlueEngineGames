// Release identity, rather than the game's optional semantic version, determines
// which publication is installed. Old launcher/installer receipts remain valid.
export function releaseDetails(game, installation) {
  const published = Array.isArray(game.versions) ? game.versions : [];
  const current = published.find((v) => v.release === game.package.release);
  const meaningfulVersion = (value) =>
    value && value !== "catalog" ? value : null;
  const latest = {
    ...current,
    release: game.package.release,
    name:
      meaningfulVersion(game.version) || current?.name || game.package.release,
    released_at: game.released_at || current?.released_at || null,
    latest: true,
  };
  const versions = [
    latest,
    ...published.filter((v) => v.release !== latest.release),
  ];
  if (!installation?.installed) return { latest, installed: null, versions };
  const match = versions.find((v) => v.release === installation.release);
  return {
    latest,
    versions,
    installed: {
      release: installation.release,
      name:
        meaningfulVersion(installation.version) ||
        match?.name ||
        installation.release ||
        "Unknown version",
      released_at: installation.released_at || match?.released_at || null,
      installed_at: installation.installed_at ?? null,
    },
  };
}

export function versionLabel(value) {
  return /^\d/.test(value || "") ? `v${value}` : value || "Unknown version";
}

export function releaseDate(value) {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}T/.test(value))
    return null;
  const date = new Date(value);
  return Number.isFinite(date.getTime())
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeZone: "UTC",
      }).format(date)
    : null;
}

export function installationDate(value) {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0)
    return null;
  const date = new Date(value);
  return Number.isFinite(date.getTime())
    ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date)
    : null;
}
