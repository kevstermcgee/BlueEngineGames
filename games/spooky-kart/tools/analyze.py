#!/usr/bin/env python3
"""Summarise a Spooky Kart server's races.jsonl: which characters win and why, and how the network did.

  python tools/analyze.py [races.jsonl ...] [--json]

Each line is one finished race (results, per-racer statistics, per-player network quality, server load).
Read the character table to rebalance src/character.rs (a character far from the average place needs a
nudge) and the network table to find lag, loss or a slow server. Numbers from bots-only races say little
about handling and drifting; races with people say more.
"""
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path


def load(paths):
    races = []
    for path in paths:
        for line in Path(path).read_text().splitlines():
            if line.strip():
                races.append(json.loads(line))
    return races


def mean(values):
    values = list(values)
    return statistics.fmean(values) if values else 0.0


def summarise(races):
    per = defaultdict(lambda: defaultdict(list))
    for race in races:
        for r in race["race"]["racers"]:
            c = r["character"]
            per[c]["place"].append(r["place"])
            per[c]["win"].append(1 if r["place"] == 1 else 0)
            per[c]["human"].append(1 if r["human"] else 0)
            if r["finish_seconds"] is not None:
                per[c]["finish"].append(r["finish_seconds"])
            if r["best_lap_seconds"] is not None:
                per[c]["best_lap"].append(r["best_lap_seconds"])
            for key in ("drift_seconds", "offroad_seconds", "wall_hits", "collisions", "perk_uses", "hazard_hits", "top_speed", "mean_speed"):
                per[c][key].append(r[key])
    characters = {}
    for c, d in per.items():
        n = len(d["place"])
        characters[c] = {
            "races": n,
            "win_rate": mean(d["win"]),
            "mean_place": mean(d["place"]),
            "mean_finish_s": mean(d["finish"]),
            "best_lap_s": min(d["best_lap"]) if d["best_lap"] else None,
            "human_share": mean(d["human"]),
            **{k: mean(d[k]) for k in ("drift_seconds", "offroad_seconds", "wall_hits", "collisions", "perk_uses", "hazard_hits", "top_speed", "mean_speed")},
        }
    peers = [p for race in races for p in race["net"]["peers"]]
    servers = [race["net"]["server"] for race in races]
    seconds = mean(race["race"]["race_seconds"] for race in races) or 1
    network = {
        "races": len(races),
        "race_seconds_mean": seconds,
        "players": len(peers),
        "rtt_ms_mean": mean(p["rtt_ms_mean"] for p in peers if p["rtt_ms_mean"] > 0),
        "rtt_ms_max": max((p["rtt_ms_max"] for p in peers), default=0),
        "down_kb_s": mean(p["bytes_out"] / seconds / 1024 for p in peers),
        "up_kb_s": mean(p["bytes_in"] / seconds / 1024 for p in peers),
        "ticks_repeated_pct": 100 * sum(p["ticks_repeated"] for p in peers) / max(1, sum(s["ticks"] for s in servers) * max(1, len(peers) // max(1, len(races)))),
        "inputs_skipped": sum(p["inputs_skipped"] for p in peers),
        "inputs_late": sum(p["inputs_late"] for p in peers),
        "left_early": sum(1 for p in peers if p["left_early"]),
        "server_tick_us_mean": mean(s["tick_us_mean"] for s in servers),
        "server_tick_us_max": max((s["tick_us_max"] for s in servers), default=0),
        "snapshot_bytes_max": max((s["snapshot_bytes_max"] for s in servers), default=0),
        "bad_datagrams": sum(s["bad_datagrams"] for s in servers),
    }
    return characters, network


def main(argv):
    as_json = "--json" in argv
    paths = [a for a in argv if not a.startswith("--")] or ["spooky-kart-data/races.jsonl"]
    races = load(paths)
    if not races:
        print("no races yet")
        return 1
    characters, network = summarise(races)
    if as_json:
        print(json.dumps({"characters": characters, "network": network}, indent=2))
        return 0
    print(f"{len(races)} race(s)\n")
    print(f"{'character':<24}{'races':>6}{'wins':>7}{'place':>7}{'finish':>8}{'drift s':>9}{'walls':>7}{'bumps':>7}{'perks':>7}{'hazard':>7}{'top m/s':>9}")
    average = mean(c["mean_place"] for c in characters.values())
    for name, c in sorted(characters.items(), key=lambda kv: kv[1]["mean_place"]):
        flag = "  <- strong" if c["mean_place"] < average - 1.2 else ("  <- weak" if c["mean_place"] > average + 1.2 else "")
        print(f"{name:<24}{c['races']:>6}{c['win_rate']*100:>6.0f}%{c['mean_place']:>7.2f}{c['mean_finish_s']:>7.1f}s{c['drift_seconds']:>9.1f}{c['wall_hits']:>7.1f}{c['collisions']:>7.1f}{c['perk_uses']:>7.1f}{c['hazard_hits']:>7.1f}{c['top_speed']:>9.1f}{flag}")
    n = network
    print(f"\nnetwork over {n['players']} player-races: rtt {n['rtt_ms_mean']:.0f} ms mean / {n['rtt_ms_max']} max, "
          f"{n['down_kb_s']:.1f} KB/s down, {n['up_kb_s']:.1f} KB/s up, {n['ticks_repeated_pct']:.2f}% ticks on a repeated input, "
          f"{n['inputs_skipped']} inputs skipped, {n['inputs_late']} late, {n['left_early']} left early")
    print(f"server: tick {n['server_tick_us_mean']:.0f} us mean / {n['server_tick_us_max']} max, largest snapshot {n['snapshot_bytes_max']} B, {n['bad_datagrams']} bad datagrams")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
