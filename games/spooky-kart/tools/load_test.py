#!/usr/bin/env python3
"""Load-test the real server over real UDP with bot clients, and print metric rows.

  python tools/load_test.py [--clients 1 4 8] [--races 1] [--release] [--out FILE.jsonl]

For each client count it starts spooky-kart-server on loopback, runs spooky-kart-bots for the requested
number of full races (about two minutes each, in real time), then reads the server's races.jsonl. It prints one
JSON metric row per line: {"metric", "kind", "profile", "value", "unit", ...}, the schema BlueEngine's
tools/perf.py records into docs/perf/metrics.jsonl (`python tools/perf.py record --suite kart`).
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import analyze  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
TICK = os.sysconf("SC_CLK_TCK")


def cpu_seconds(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / TICK


def rss_peak_mb(pid):
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith("VmHWM:"):
            return round(int(line.split()[1]) / 1024, 1)
    return None


def build(release):
    cmd = ["cargo", "build", "--no-default-features", "--bin", "spooky-kart-server", "--bin", "spooky-kart-bots"]
    if release:
        cmd.insert(2, "--release")
    done = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if done.returncode:
        sys.exit(done.stderr[-2000:])
    return ROOT / "target" / ("release" if release else "debug")


def run_level(bin_dir, clients, races, port, profile):
    data = Path(tempfile.mkdtemp(prefix="spooky-kart-load-"))
    server = subprocess.Popen(
        [str(bin_dir / "spooky-kart-server"), "--listen", f"127.0.0.1:{port}", "--report-dir", str(data), "--auto-start", "0"],
        stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
    try:
        time.sleep(1.0)
        before, wall = cpu_seconds(server.pid), time.monotonic()
        bots = subprocess.run(
            [str(bin_dir / "spooky-kart-bots"), f"127.0.0.1:{port}", "--clients", str(clients), "--races", str(races), "--seconds", str(races * 240 + 60)],
            capture_output=True, text=True)
        busy, window = cpu_seconds(server.pid) - before, time.monotonic() - wall
        rss = rss_peak_mb(server.pid)
    finally:
        server.terminate()
        server.wait(timeout=10)
    if bots.returncode:
        sys.exit(f"bots failed at {clients} clients: {bots.stderr[-800:]}")
    report = json.loads(bots.stdout.strip().splitlines()[-1])
    log = data / "races.jsonl"
    if not log.exists():
        sys.exit("the server logged no race")
    races_data = analyze.load([log])
    characters, network = analyze.summarise(races_data)
    kind = f"clients_{clients}"
    common = {"kind": kind, "profile": profile, "game": "spooky-kart"}
    row = lambda metric, value, unit: {"metric": metric, "value": round(value, 3) if isinstance(value, float) else value, "unit": unit, **common}
    clients_report = report["per_client"]
    rows = [
        row("kart_server_cpu_pct_of_core", 100 * busy / window, "%"),
        row("kart_server_rss_peak", rss, "MB"),
        row("kart_server_tick_mean", network["server_tick_us_mean"], "us"),
        row("kart_server_tick_max", network["server_tick_us_max"], "us"),
        row("kart_snapshot_bytes_max", network["snapshot_bytes_max"], "B"),
        row("kart_down_kb_per_s", network["down_kb_s"], "KB/s"),
        row("kart_up_kb_per_s", network["up_kb_s"], "KB/s"),
        row("kart_rtt_mean", network["rtt_ms_mean"], "ms"),
        row("kart_rtt_max", network["rtt_ms_max"], "ms"),
        row("kart_ticks_repeated_pct", network["ticks_repeated_pct"], "%"),
        row("kart_inputs_skipped", network["inputs_skipped"], "n"),
        row("kart_race_seconds", network["race_seconds_mean"], "s"),
        row("kart_client_corrections", sum(c["corrections"] for c in clients_report), "n"),
        row("kart_client_snaps", sum(c["snaps"] for c in clients_report), "n"),
        row("kart_client_max_error", max(c["max_error_m"] for c in clients_report), "m"),
    ]
    for name, c in characters.items():
        rows.append({**row("kart_character_mean_place", c["mean_place"], "place"), "character": name})
        rows.append({**row("kart_character_win_rate", c["win_rate"], "fraction"), "character": name})
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--clients", type=int, nargs="+", default=[1, 4, 8])
    p.add_argument("--races", type=int, default=1)
    p.add_argument("--release", action="store_true")
    p.add_argument("--out")
    args = p.parse_args()
    bin_dir = build(args.release)
    profile = "release" if args.release else "dev"
    rows = []
    for index, clients in enumerate(args.clients):
        rows += run_level(bin_dir, clients, args.races, 41700 + index, profile)
    out = open(args.out, "a") if args.out else None
    for row in rows:
        line = json.dumps(row, sort_keys=True)
        print(line)
        if out:
            out.write(line + "\n")


if __name__ == "__main__":
    main()
