"""HLTV data fetcher.

Fetch strategy (tried in order, first success wins):
  1. hltvorg-api  — community Python library that uses Selenium/Firefox to bypass
                    Cloudflare. Gives real team rankings + rosters. Requires:
                    ``pip install hltvorg-api selenium pyvirtualdisplay`` and
                    Firefox + geckodriver on PATH.
  2. Requests scrape — plain HTTP GET; fast but blocked by Cloudflare most of the time.
  3. Synthetic fallback — built-in 2025 CS2 data used when both live sources fail.

When source 3 is used the teams.json cache is written with ``"synthetic": true`` and
the GUI shows a blue notice banner.
"""
import json
import random
import logging
from pathlib import Path

from .cache import cache_write

logger = logging.getLogger(__name__)

# Directory that holds overrides.json (same level as data/cache/)
_DATA_DIR = Path(__file__).parent.parent / "data"

# ── Name normalisation ────────────────────────────────────────────────────────
# Maps live HLTV names → canonical short names used in the GUI and overrides.
# Add entries here whenever the live feed returns a confusing full name.
_NAME_ALIASES: dict[str, str] = {
    "Natus Vincere": "NAVI",
    "HEROIC":        "Heroic",
}

# Current CS2 competitive map pool.  Update this list whenever Valve rotates a map;
# the value is written to data/cache/teams.json on every fetch so the Rust GUI
# picks it up automatically without a recompile.
ACTIVE_MAPS = ["Mirage", "Dust2", "Nuke", "Overpass", "Anubis", "Ancient", "Inferno"]

# Top-30 CS2 team data — 2025 standings
_SYNTHETIC_TEAMS = [
    {"name": "G2",             "ranking": 1,  "avg_rating": 1.16, "recent_wr_1m": 0.74, "recent_wr_3m": 0.71, "recent_wr_6m": 0.68},
    {"name": "Spirit",         "ranking": 2,  "avg_rating": 1.14, "recent_wr_1m": 0.72, "recent_wr_3m": 0.69, "recent_wr_6m": 0.67},
    {"name": "Vitality",       "ranking": 3,  "avg_rating": 1.12, "recent_wr_1m": 0.70, "recent_wr_3m": 0.68, "recent_wr_6m": 0.65},
    {"name": "Falcons",        "ranking": 4,  "avg_rating": 1.11, "recent_wr_1m": 0.69, "recent_wr_3m": 0.67, "recent_wr_6m": 0.64},
    {"name": "MOUZ",           "ranking": 5,  "avg_rating": 1.10, "recent_wr_1m": 0.68, "recent_wr_3m": 0.65, "recent_wr_6m": 0.63},
    {"name": "FaZe",           "ranking": 6,  "avg_rating": 1.09, "recent_wr_1m": 0.67, "recent_wr_3m": 0.64, "recent_wr_6m": 0.62},
    {"name": "NAVI",           "ranking": 7,  "avg_rating": 1.08, "recent_wr_1m": 0.65, "recent_wr_3m": 0.63, "recent_wr_6m": 0.61},
    {"name": "Heroic",         "ranking": 8,  "avg_rating": 1.06, "recent_wr_1m": 0.63, "recent_wr_3m": 0.61, "recent_wr_6m": 0.59},
    {"name": "NIP",            "ranking": 9,  "avg_rating": 1.05, "recent_wr_1m": 0.62, "recent_wr_3m": 0.60, "recent_wr_6m": 0.58},
    {"name": "Virtus.pro",     "ranking": 10, "avg_rating": 1.06, "recent_wr_1m": 0.63, "recent_wr_3m": 0.61, "recent_wr_6m": 0.59},
    {"name": "Liquid",         "ranking": 11, "avg_rating": 1.04, "recent_wr_1m": 0.61, "recent_wr_3m": 0.59, "recent_wr_6m": 0.57},
    {"name": "3DMAX",          "ranking": 12, "avg_rating": 1.04, "recent_wr_1m": 0.61, "recent_wr_3m": 0.59, "recent_wr_6m": 0.58},
    {"name": "FURIA",          "ranking": 13, "avg_rating": 1.03, "recent_wr_1m": 0.60, "recent_wr_3m": 0.58, "recent_wr_6m": 0.57},
    {"name": "Cloud9",         "ranking": 14, "avg_rating": 1.03, "recent_wr_1m": 0.59, "recent_wr_3m": 0.58, "recent_wr_6m": 0.56},
    {"name": "GamerLegion",    "ranking": 15, "avg_rating": 1.02, "recent_wr_1m": 0.58, "recent_wr_3m": 0.57, "recent_wr_6m": 0.55},
    {"name": "Astralis",       "ranking": 16, "avg_rating": 1.02, "recent_wr_1m": 0.58, "recent_wr_3m": 0.57, "recent_wr_6m": 0.56},
    {"name": "ENCE",           "ranking": 17, "avg_rating": 1.01, "recent_wr_1m": 0.57, "recent_wr_3m": 0.56, "recent_wr_6m": 0.55},
    {"name": "BIG",            "ranking": 18, "avg_rating": 1.02, "recent_wr_1m": 0.58, "recent_wr_3m": 0.57, "recent_wr_6m": 0.55},
    {"name": "paiN",           "ranking": 19, "avg_rating": 1.01, "recent_wr_1m": 0.57, "recent_wr_3m": 0.56, "recent_wr_6m": 0.54},
    {"name": "Imperial",       "ranking": 20, "avg_rating": 1.01, "recent_wr_1m": 0.56, "recent_wr_3m": 0.55, "recent_wr_6m": 0.54},
    {"name": "Complexity",     "ranking": 21, "avg_rating": 1.00, "recent_wr_1m": 0.55, "recent_wr_3m": 0.54, "recent_wr_6m": 0.53},
    {"name": "EG",             "ranking": 22, "avg_rating": 1.00, "recent_wr_1m": 0.55, "recent_wr_3m": 0.53, "recent_wr_6m": 0.52},
    {"name": "Monte",          "ranking": 23, "avg_rating": 0.99, "recent_wr_1m": 0.54, "recent_wr_3m": 0.53, "recent_wr_6m": 0.52},
    {"name": "OG",             "ranking": 24, "avg_rating": 0.99, "recent_wr_1m": 0.53, "recent_wr_3m": 0.52, "recent_wr_6m": 0.51},
    {"name": "Lynn Vision",    "ranking": 25, "avg_rating": 0.98, "recent_wr_1m": 0.52, "recent_wr_3m": 0.51, "recent_wr_6m": 0.50},
    {"name": "TYLOO",          "ranking": 26, "avg_rating": 0.97, "recent_wr_1m": 0.51, "recent_wr_3m": 0.50, "recent_wr_6m": 0.49},
    {"name": "Grayhound",      "ranking": 27, "avg_rating": 0.97, "recent_wr_1m": 0.51, "recent_wr_3m": 0.50, "recent_wr_6m": 0.49},
    {"name": "Bad News Eagles","ranking": 28, "avg_rating": 0.98, "recent_wr_1m": 0.52, "recent_wr_3m": 0.51, "recent_wr_6m": 0.50},
    {"name": "9INE",           "ranking": 29, "avg_rating": 0.97, "recent_wr_1m": 0.51, "recent_wr_3m": 0.50, "recent_wr_6m": 0.49},
    {"name": "AMKAL",          "ranking": 30, "avg_rating": 0.96, "recent_wr_1m": 0.50, "recent_wr_3m": 0.49, "recent_wr_6m": 0.48},
]

# Fill remaining teams with generic players
def _gen_players(team_name: str, avg_rating: float, n: int = 5) -> list:
    rng = random.Random(hash(team_name) & 0xFFFFFFFF)
    players = []
    for i in range(n):
        r = avg_rating + rng.gauss(0, 0.05)
        players.append((
            f"{team_name}_P{i+1}",
            round(max(0.85, r), 3),
            round(max(0.85, r + rng.gauss(0, 0.03)), 3),
            round(50 + (r - 1.0) * 80 + rng.gauss(0, 3), 1),
            round(68 + (r - 1.0) * 25 + rng.gauss(0, 2), 1),
            round(0.47 + (r - 1.0) * 0.1 + rng.gauss(0, 0.02), 3),
        ))
    return players


def _gen_team_for_rank(rank: int) -> dict:
    """Generate a synthetic team entry for a rank not in the hardcoded list.

    Stats decline smoothly with rank so the model sees sensible relative strength.
    Names are placeholders — they will be overwritten by live data when available.
    """
    avg_rating  = round(max(0.92, 1.16 - (rank - 1) * 0.004), 3)
    wr_base     = round(max(0.44, 0.74 - (rank - 1) * 0.004), 3)
    return {
        "name":         f"Team_{rank}",
        "ranking":      rank,
        "avg_rating":   avg_rating,
        "recent_wr_1m": round(min(0.95, wr_base + 0.02), 3),
        "recent_wr_3m": wr_base,
        "recent_wr_6m": round(max(0.40, wr_base - 0.02), 3),
    }


def _build_synthetic_data(count: int = 30) -> tuple[list, list]:
    # Build the full list of team templates: use the hardcoded list for ranks
    # 1-30 and generate entries on-the-fly for any ranks beyond that.
    all_templates = list(_SYNTHETIC_TEAMS)
    for rank in range(len(_SYNTHETIC_TEAMS) + 1, count + 1):
        all_templates.append(_gen_team_for_rank(rank))

    templates = all_templates[:count]

    teams_out = []
    players_out = []

    for t in templates:
        rng = random.Random(hash(t["name"]) & 0xFFFFFFFF)

        # Each team has 5-7 maps in their pool
        pool_size = rng.randint(5, 7)
        shuffled = ACTIVE_MAPS[:]
        rng.shuffle(shuffled)
        pool = shuffled[:pool_size]

        map_win_rates = {}
        for m in ACTIVE_MAPS:
            base = t["recent_wr_3m"]
            if m in pool:
                map_win_rates[m] = min(0.95, base + rng.uniform(0.03, 0.10))
            else:
                map_win_rates[m] = max(0.30, base - rng.uniform(0.05, 0.18))

        h2h = {}
        for other in templates:
            if other["name"] == t["name"]:
                continue
            delta = t["avg_rating"] - other["avg_rating"]
            h2h[other["name"]] = round(0.5 + delta * 1.2 + rng.gauss(0, 0.04), 3)
            h2h[other["name"]] = max(0.2, min(0.8, h2h[other["name"]]))

        teams_out.append({
            "name":         t["name"],
            "ranking":      t["ranking"],
            "avg_rating":   t["avg_rating"],
            "recent_wr_1m": t["recent_wr_1m"],
            "recent_wr_3m": t["recent_wr_3m"],
            "recent_wr_6m": t["recent_wr_6m"],
            "map_pool":     pool,
            "map_win_rates": map_win_rates,
            "h2h":          h2h,
        })

        raw = _gen_players(t["name"], t["avg_rating"])
        for pname, rating, kd, adr, kast, od_success in raw:
            players_out.append({
                "name":                 pname,
                "team":                 t["name"],
                "_rank":                t["ranking"],   # join-key: never collides during rename
                "rating":               rating,
                "kd":                   kd,
                "adr":                  adr,
                "kast":                 kast,
                "opening_duel_success": od_success,
                "map_ratings": {
                    m: round(rating + rng.gauss(0, 0.08), 3) for m in ACTIVE_MAPS
                },
            })

    return teams_out, players_out


def _build_synthetic_matches(teams: list) -> list:
    rng = random.Random(42)
    team_map = {t["name"]: t for t in teams}
    matches = []

    # Scale sample count with pool size so each team pair is seen ~10 times.
    # n_teams=30 → 2000, n_teams=50 → 4000, n_teams=100 → 10000 (capped at 15000)
    n = min(15_000, max(2_000, len(teams) * len(teams) * 2))

    for _ in range(n):
        t_a, t_b = rng.sample(teams, 2)
        map_name = rng.choice(ACTIVE_MAPS)

        # Outcome weighted by rating + map win rate
        p_a = 0.5 + (t_a["avg_rating"] - t_b["avg_rating"]) * 1.5
        p_a += (t_a["map_win_rates"][map_name] - t_b["map_win_rates"][map_name]) * 0.3
        p_a += (t_a["recent_wr_3m"] - t_b["recent_wr_3m"]) * 0.4
        p_a = max(0.1, min(0.9, p_a))
        winner = t_a["name"] if rng.random() < p_a else t_b["name"]

        matches.append({
            "team_a": t_a["name"],
            "team_b": t_b["name"],
            "map": map_name,
            "winner": winner,
            "team_a_rating": round(t_a["avg_rating"] + rng.gauss(0, 0.03), 4),
            "team_b_rating": round(t_b["avg_rating"] + rng.gauss(0, 0.03), 4),
            "team_a_recent_wr": round(t_a["recent_wr_3m"] + rng.gauss(0, 0.02), 4),
            "team_b_recent_wr": round(t_b["recent_wr_3m"] + rng.gauss(0, 0.02), 4),
            "h2h_wr": round(t_a["h2h"].get(t_b["name"], 0.5), 4),
            "team_a_map_wr": round(t_a["map_win_rates"][map_name], 4),
            "team_b_map_wr": round(t_b["map_win_rates"][map_name], 4),
            "map_in_a_pool": 1.0 if map_name in t_a["map_pool"] else 0.0,
            "map_in_b_pool": 1.0 if map_name in t_b["map_pool"] else 0.0,
        })
    return matches


# ── Data source 0: gigobyte/HLTV via Node.js subprocess ──────────────────────

def _try_gigobyte_hltv(count: int = 30) -> tuple[list, list] | None:
    """Call pipeline/hltv-node/fetch.mjs and parse its JSON output.

    Returns (live_teams, live_results) on success, or None if Node.js / the
    script is missing or the call fails.

    live_teams   — [{name, ranking, points, country, players:[{name,id}]}, …]
    live_results — [{team1, team2, winner, score1, score2, map, date, format}, …]

    Prerequisites:
        cd pipeline/hltv-node && npm install
    """
    import subprocess
    import json
    import shutil
    from pathlib import Path

    node_bin = shutil.which("node")
    if not node_bin:
        logger.info("node not on PATH — skipping gigobyte/HLTV (install Node.js to enable)")
        return None

    script = Path(__file__).parent / "hltv-node" / "fetch.mjs"
    if not script.exists():
        logger.info(f"fetch.mjs not found at {script} — skipping")
        return None

    node_modules = script.parent / "node_modules"
    if not node_modules.exists():
        logger.info("hltv-node/node_modules missing — run: cd pipeline/hltv-node && npm install")
        return None

    print("  Trying gigobyte/HLTV (Node.js)…")
    try:
        proc = subprocess.run(
            [node_bin, str(script), "--months", "3", "--count", str(count)],
            capture_output=True,
            text=True,
            timeout=300,  # 5 min ceiling — 30 teams × 1.5 s + 3 result pages
        )
    except subprocess.TimeoutExpired:
        logger.warning("gigobyte/HLTV timed out after 5 minutes")
        return None

    # Forward Node stderr (progress lines) to our stdout
    for line in proc.stderr.strip().splitlines():
        print(f"    [node] {line}")

    if proc.returncode != 0:
        logger.warning(f"gigobyte/HLTV exited with code {proc.returncode}")
        return None

    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        logger.warning(f"gigobyte/HLTV JSON parse error: {exc}")
        return None

    teams   = data.get("teams", [])
    results = data.get("results", [])
    if len(teams) < 10:
        logger.warning(f"gigobyte/HLTV returned only {len(teams)} teams — ignoring")
        return None

    # Annotate whether results came from getResults (series) or getMatchesStats (per-map)
    series_results = [r for r in results if r.get("format") != "map"]
    map_results    = [r for r in results if r.get("format") == "map"]
    if map_results and not series_results:
        print(f"  gigobyte/HLTV: {len(teams)} teams, {len(map_results)} map-level results (via stats endpoint).")
    else:
        print(f"  gigobyte/HLTV: {len(teams)} teams, {len(results)} results.")
    return teams, results


# ── Data source 1: hltvorg-api (Selenium-based community library) ─────────────

def _try_hltvorg_api() -> list | None:
    """Attempt to fetch top-30 teams + rosters via the hltvorg-api library.

    Returns a list of dicts with keys ``name``, ``ranking``, ``players``
    (list of player-name strings), or None if the library is unavailable or fails.

    Requires: ``pip install hltvorg-api selenium pyvirtualdisplay`` and Firefox +
    geckodriver installed on the system.
    """
    try:
        from HLTV import Teams  # type: ignore[import]
    except ImportError:
        logger.info("hltvorg-api not installed — skipping (pip install hltvorg-api selenium pyvirtualdisplay)")
        return None

    try:
        print("  Trying hltvorg-api (Selenium)…")
        result = Teams().GetTopTeams(size=30)
        if not result or not result.teams:
            logger.warning("hltvorg-api returned empty team list")
            return None

        live_teams = []
        for i, name in enumerate(result.teams):
            ranking = i + 1
            # result.players is a 2-D list: [[p1, p2, …], …] one inner list per team
            roster = result.players[i] if result.players and i < len(result.players) else []
            live_teams.append({
                "name": name,
                "ranking": ranking,
                "players": [str(p).strip() for p in roster if str(p).strip()],
            })

        print(f"  hltvorg-api: fetched {len(live_teams)} teams.")
        return live_teams if len(live_teams) >= 10 else None

    except Exception as exc:
        logger.warning(f"hltvorg-api failed: {exc}")
        return None


# ── Data source 2: plain requests scrape (often blocked by Cloudflare) ────────

def _try_requests_scrape() -> list | None:
    """Lightweight fallback: plain HTTP GET to hltv.org/ranking/teams.
    Usually blocked by Cloudflare, but worth trying when Selenium is absent.
    Returns a list of {name, ranking} dicts or None.
    """
    try:
        import requests
        from bs4 import BeautifulSoup
    except ImportError:
        return None

    _HEADERS = {
        "User-Agent": (
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
            "(KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36"
        ),
        "Accept-Language": "en-US,en;q=0.9",
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    }

    try:
        print("  Trying plain HTTP scrape…")
        r = requests.get("https://www.hltv.org/ranking/teams", headers=_HEADERS, timeout=10)
        if r.status_code != 200:
            logger.warning(f"HLTV HTTP scrape returned {r.status_code}")
            return None
        soup = BeautifulSoup(r.text, "lxml")
        teams = []
        for item in soup.select(".ranked-team")[:30]:
            name_el = item.select_one(".teamLine .name")
            rank_el = item.select_one(".position")
            if not name_el or not rank_el:
                continue
            teams.append({
                "name": name_el.text.strip(),
                "ranking": int(rank_el.text.strip().lstrip("#")),
                "players": [],
            })
        if len(teams) >= 10:
            print(f"  HTTP scrape: fetched {len(teams)} teams.")
            return teams
        return None
    except Exception as exc:
        logger.warning(f"HTTP scrape failed: {exc}")
        return None


# ── Merge live data into synthetic base ───────────────────────────────────────

def _dedup_players(teams: list, players: list) -> None:
    """Remove duplicate player names across teams.

    HLTV roster data sometimes shows a recently-transferred player on two teams
    simultaneously.  We keep the player on the better-ranked team (lower ranking
    number) and replace the stale duplicate with a generated placeholder so
    every team retains exactly the right number of players.
    """
    rank_of = {t["name"]: t["ranking"] for t in teams}
    seen: dict[str, str] = {}          # player_name → best_team_name

    # First pass: record the best (lowest-rank = strongest) team per name
    for p in players:
        name = p["name"]
        team = p["team"]
        # Skip placeholder names like "TeamName_P3"
        if "_P" in name and name.split("_P")[-1].isdigit():
            continue
        best = seen.get(name)
        if best is None or rank_of.get(team, 999) < rank_of.get(best, 999):
            seen[name] = team

    # Second pass: replace stale duplicates with numbered placeholders
    rng = random.Random(0xDEAD)
    for p in players:
        name = p["name"]
        if "_P" in name and name.split("_P")[-1].isdigit():
            continue
        if seen.get(name) != p["team"]:
            # This is the stale copy — generate a unique placeholder name
            p["name"] = f"{p['team']}_sub{rng.randint(10, 99)}"


def _apply_live_data(teams: list, players: list, live_teams: list) -> None:
    """Patch ``teams`` and ``players`` in-place using real names/rosters from
    ``live_teams`` (output of either live source above).

    - Team names are normalised through ``_NAME_ALIASES`` before being applied.
    - Rankings are updated to the live positions.
    - When the live source provides a roster, synthetic player records are
      renamed to real player names in descending-rating order.

    **Important**: players are looked up by ``_rank`` (stamped at creation),
    NOT by team-name string.  Renaming by name causes cascade collisions when
    the new names happen to match another team's current name (e.g. live rank-1
    = "Vitality" renames the rank-1 players to team="Vitality"; then live rank-3
    = "Spirit" updating the rank-3 team (originally named "Vitality") would
    see *both* sets of "Vitality" players and steal them all).
    """
    name_by_rank   = {t["ranking"]: _NAME_ALIASES.get(t["name"], t["name"]) for t in live_teams}
    roster_by_rank = {t["ranking"]: t.get("players", []) for t in live_teams}

    def _pname(entry: object) -> str:
        return entry["name"] if isinstance(entry, dict) else str(entry)

    for team in teams:
        rank = team["ranking"]
        if rank not in name_by_rank:
            continue

        new_name = name_by_rank[rank]
        team["name"] = new_name

        # ── Locate players by rank, not by name ──────────────────────────────
        team_players = [p for p in players if p.get("_rank") == rank]

        for p in team_players:
            p["team"] = new_name

        # Overwrite synthetic player names with real HLTV starter names.
        live_roster = roster_by_rank.get(rank) or []
        if live_roster:
            ranked = sorted(team_players, key=lambda p: p["rating"], reverse=True)
            for idx, entry in enumerate(live_roster[:len(ranked)]):
                ranked[idx]["name"] = _pname(entry)


# ── Convert real results into training-match format ───────────────────────────

def _build_matches_from_results(live_results: list, teams: list) -> list:
    """Turn gigobyte/HLTV result records into the feature-vector training format.

    Any match whose teams are not both in our top-30 set is dropped.
    If fewer than 300 real matches survive, synthetic matches are appended to
    ensure the model has enough training signal.
    """
    team_map = {t["name"]: t for t in teams}
    matches = []

    for r in live_results:
        t_a = team_map.get(r["team1"])
        t_b = team_map.get(r["team2"])
        if not t_a or not t_b:
            continue

        map_name = r.get("map") if r.get("map") in ACTIVE_MAPS else random.choice(ACTIVE_MAPS)

        matches.append({
            "team_a":           r["team1"],
            "team_b":           r["team2"],
            "map":              map_name,
            "winner":           r["winner"],
            "team_a_rating":    round(t_a["avg_rating"], 4),
            "team_b_rating":    round(t_b["avg_rating"], 4),
            "team_a_recent_wr": round(t_a["recent_wr_3m"], 4),
            "team_b_recent_wr": round(t_b["recent_wr_3m"], 4),
            "h2h_wr":           round(t_a["h2h"].get(r["team2"], 0.5), 4),
            "team_a_map_wr":    round(t_a["map_win_rates"].get(map_name, t_a["recent_wr_3m"]), 4),
            "team_b_map_wr":    round(t_b["map_win_rates"].get(map_name, t_b["recent_wr_3m"]), 4),
            "map_in_a_pool":    1.0 if map_name in t_a["map_pool"] else 0.0,
            "map_in_b_pool":    1.0 if map_name in t_b["map_pool"] else 0.0,
        })

    # Pad with synthetic data if real results are sparse.
    # Target: at least as many samples as pure-synthetic would produce so the
    # model always has enough signal.  Real results are placed first so the
    # model sees them in every epoch (before the synthetic padding).
    target = max(300, min(15_000, len(teams) * len(teams) * 2))
    if len(matches) < target:
        if matches:
            logger.info(f"  {len(matches)} real results — padding with synthetic to reach {target}.")
        else:
            logger.info(f"  0 real results — training on {target} synthetic matches.")
        matches += _build_synthetic_matches(teams)

    return matches


# ── Manual overrides (data/overrides.json) ───────────────────────────────────

_OVERRIDES_PATH = _DATA_DIR / "overrides.json"

_OVERRIDES_TEMPLATE = {
    "_doc": (
        "Manual overrides — merged on top of live/synthetic data every fetch run. "
        "Edit freely. Team names must match exactly what appears in the GUI "
        "(check data/cache/teams.json after a fetch to see current names). "
        "All fields are optional: omit what you don't want to override."
    ),
    "teams": {
        "_doc": "Override per-team statistics. Keys are team names.",
        "Falcons": {
            "_doc": "Roster was Cloudflare-blocked on last fetch — stats estimated from ranking",
            "avg_rating":    1.11,
            "recent_wr_1m":  0.69,
            "recent_wr_3m":  0.67,
            "recent_wr_6m":  0.64,
        },
        "The MongolZ": {
            "_doc": "Roster was Cloudflare-blocked on last fetch — stats estimated from ranking",
            "avg_rating":    1.07,
            "recent_wr_1m":  0.64,
            "recent_wr_3m":  0.62,
            "recent_wr_6m":  0.60,
        },
        "TYLOO": {
            "_doc": "Roster was Cloudflare-blocked on last fetch — stats estimated from ranking",
            "avg_rating":    0.97,
            "recent_wr_1m":  0.51,
            "recent_wr_3m":  0.50,
            "recent_wr_6m":  0.49,
        },
    },
    "players": {
        "_doc": (
            "Override player rosters. Each key is a team name; value is a list of "
            "[name, rating, kd, adr, kast, od_success]. "
            "Replaces the entire roster for that team."
        ),
        "Falcons": [
            ["Twistzz",  1.19, 1.23, 77.2, 74.0, 0.53],
            ["FalleN",   1.09, 1.11, 70.5, 72.6, 0.51],
            ["YEKINDAR", 1.15, 1.18, 74.2, 73.5, 0.52],
            ["Spinx",    1.12, 1.14, 72.8, 73.1, 0.51],
            ["dupreeh",  1.06, 1.08, 68.3, 72.0, 0.50],
        ],
        "The MongolZ": [
            ["Techno",   1.14, 1.17, 73.5, 73.4, 0.52],
            ["bLitz",    1.10, 1.12, 71.0, 72.7, 0.51],
            ["Senzu",    1.08, 1.10, 69.8, 72.4, 0.50],
            ["mzinho",   1.06, 1.08, 68.5, 72.1, 0.50],
            ["Xlnt",     1.04, 1.05, 67.0, 71.8, 0.50],
        ],
        "TYLOO": [
            ["Summer",    1.02, 1.03, 65.5, 71.5, 0.49],
            ["Attacker",  0.99, 0.99, 63.0, 70.8, 0.48],
            ["somebody",  0.98, 0.98, 62.5, 70.5, 0.48],
            ["JamYoung",  0.97, 0.97, 62.0, 70.3, 0.47],
            ["afufu",     0.96, 0.96, 61.5, 70.0, 0.47],
        ],
    },
    "results": {
        "_doc": (
            "Extra match results for model training. Useful when the results fetch "
            "is blocked. Format: {team_a, team_b, winner, map (optional)}. "
            "map must be one of the ACTIVE_MAPS or omit for a random pick."
        ),
        "matches": [],
    },
}


def _load_overrides() -> dict:
    """Load data/overrides.json, creating a template if it doesn't exist yet."""
    if not _OVERRIDES_PATH.exists():
        _OVERRIDES_PATH.parent.mkdir(parents=True, exist_ok=True)
        with open(_OVERRIDES_PATH, "w") as f:
            json.dump(_OVERRIDES_TEMPLATE, f, indent=2)
        print(f"  Created {_OVERRIDES_PATH} — edit it to add manual team/player data.")
        return _OVERRIDES_TEMPLATE

    with open(_OVERRIDES_PATH) as f:
        return json.load(f)


def _apply_overrides(teams: list, players: list, overrides: dict) -> None:
    """Apply data/overrides.json on top of the assembled teams/players lists.

    Overrides take priority over everything (live and synthetic).
    Matching is by exact team name (case-sensitive).
    """
    team_map = {t["name"]: t for t in teams}

    # ── Team stat overrides ───────────────────────────────────────────────────
    for team_name, stats in overrides.get("teams", {}).items():
        if team_name.startswith("_"):
            continue
        t = team_map.get(team_name)
        if t is None:
            logger.warning(f"overrides.json: team '{team_name}' not found in cache — skipping")
            continue
        for field in ("avg_rating", "recent_wr_1m", "recent_wr_3m", "recent_wr_6m"):
            if field in stats:
                t[field] = stats[field]

    # ── Player roster overrides ───────────────────────────────────────────────
    rng = random.Random(0)
    for team_name, roster in overrides.get("players", {}).items():
        if team_name.startswith("_"):
            continue
        if team_name not in team_map:
            logger.warning(f"overrides.json: players team '{team_name}' not found — skipping")
            continue

        # Remove existing players for this team
        players[:] = [p for p in players if p["team"] != team_name]

        for entry in roster:
            # Support both array [name, r, kd, adr, kast, od] (Python template)
            # and object {"name":…, "rating":…, …} (written by Rust GUI editor)
            if isinstance(entry, (list, tuple)):
                name, rating, kd, adr, kast, od_success = entry
            else:
                name       = entry["name"]
                rating     = entry["rating"]
                kd         = entry["kd"]
                adr        = entry["adr"]
                kast       = entry["kast"]
                od_success = entry["od_success"]
            players.append({
                "name":                name,
                "team":                team_name,
                "rating":              rating,
                "kd":                  kd,
                "adr":                 adr,
                "kast":                kast,
                "opening_duel_success": od_success,
                "map_ratings": {
                    m: round(rating + rng.gauss(0, 0.08), 3) for m in ACTIVE_MAPS
                },
            })

    n_overridden = (
        sum(1 for k in overrides.get("teams", {}) if not k.startswith("_")) +
        sum(1 for k in overrides.get("players", {}) if not k.startswith("_"))
    )
    if n_overridden:
        print(f"  Applied overrides for {n_overridden} team(s) from {_OVERRIDES_PATH.name}.")


# ── Data source: PandaScore API (free tier — requires a token) ───────────────
#
# Sign up for free at https://pandascore.co/  →  My Account  →  API Access Token
# Store the token in data/pandascore_token.txt  OR  set env var PANDASCORE_TOKEN.
# Free tier: 100 requests/hour — more than enough for this pipeline.

_PANDASCORE_TOKEN_FILE = _DATA_DIR / "pandascore_token.txt"


def _load_pandascore_token() -> str | None:
    """Return a PandaScore API token from env var or token file, or None."""
    import os
    token = os.environ.get("PANDASCORE_TOKEN", "").strip()
    if token:
        return token
    if _PANDASCORE_TOKEN_FILE.exists():
        token = _PANDASCORE_TOKEN_FILE.read_text().strip()
        if token:
            return token
    return None


def _name_normalizer(team_names: set) -> "Callable[[str], str | None]":  # type: ignore[name-defined]
    """Return a function that maps arbitrary team names to our canonical names.

    Tries (in order): exact → case-insensitive → strip common affixes
    ("Team X"/"X Esports"/"X Gaming") → substring containment.
    """
    name_lower: dict[str, str] = {n.lower(): n for n in team_names}
    _SUFFIXES = [" esports", " gaming", " e-sports", " team", " cs"]
    _PREFIXES = ["team "]

    def _strip(s: str) -> str:
        for sfx in _SUFFIXES:
            if s.endswith(sfx):
                return s[: -len(sfx)]
        for pfx in _PREFIXES:
            if s.startswith(pfx):
                return s[len(pfx):]
        return s

    def normalize(raw: str) -> "str | None":
        n = (raw or "").strip()
        if not n:
            return None
        lc = n.lower()
        if n in team_names:
            return n
        if lc in name_lower:
            return name_lower[lc]
        stripped = _strip(lc)
        if stripped in name_lower:
            return name_lower[stripped]
        for kl, kn in name_lower.items():
            if _strip(kl) == stripped or _strip(kl) == lc:
                return kn
        for kl, kn in name_lower.items():
            if kl in lc or lc in kl:
                return kn
        return None

    return normalize


def _try_pandascore_results(team_names: set, token: str, months: int = 3) -> list:
    """Fetch CS2 match results from the PandaScore REST API.

    Requires a free API token — see pandascore.co.  Returns a list of result
    dicts compatible with ``_build_matches_from_results``.

    Each finished game (map) is returned as a separate record so the model
    gets richer per-map training signal (same as getMatchesStats would give).
    """
    import urllib.request
    import urllib.parse
    import json
    from datetime import datetime, timedelta, timezone

    normalize = _name_normalizer(team_names)

    since_dt = datetime.now(timezone.utc) - timedelta(days=months * 31)
    since_str = since_dt.strftime("%Y-%m-%dT%H:%M:%SZ")

    results: list[dict] = []
    page = 1
    total_series = 0
    skipped = 0

    print("  Trying PandaScore API…")
    while page <= 10:           # max 1000 series (100 per page × 10 pages)
        params = {
            "token":       token,
            "per_page":    "100",
            "page":        str(page),
            "sort":        "-begin_at",
            # Filter to matches that ended after `since` (range: since,now)
            "range[end_at]": f"{since_str},{datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}",
        }
        url = "https://api.pandascore.co/csgo/matches/past?" + urllib.parse.urlencode(params)
        try:
            req = urllib.request.Request(url, headers={
                "User-Agent": "CS2TournamentSimulator/1.0",
                "Accept":     "application/json",
            })
            with urllib.request.urlopen(req, timeout=15) as resp:
                matches = json.loads(resp.read())
        except Exception as exc:
            logger.warning(f"PandaScore page {page} failed: {exc}")
            break

        if not matches:
            break

        for m in matches:
            opponents = m.get("opponents") or []
            if len(opponents) != 2:
                continue

            t1_raw = (opponents[0].get("opponent") or {}).get("name", "")
            t2_raw = (opponents[1].get("opponent") or {}).get("name", "")
            t1_id  = (opponents[0].get("opponent") or {}).get("id")
            winner_obj = m.get("winner") or {}
            winner_raw = winner_obj.get("name", "")

            t1     = normalize(t1_raw)
            t2     = normalize(t2_raw)
            winner = normalize(winner_raw)

            if not t1 or not t2 or not winner:
                skipped += 1
                continue

            total_series += 1

            # Prefer per-game (per-map) records for richer training signal
            games = [g for g in (m.get("games") or []) if g.get("finished")]
            if games:
                for g in games:
                    g_winner_id = (g.get("winner") or {}).get("id")
                    g_winner = t1 if g_winner_id == t1_id else t2
                    map_obj  = g.get("map") or {}
                    map_raw  = map_obj.get("name", "") if isinstance(map_obj, dict) else ""
                    # PandaScore map names e.g. "de_dust2", "de_mirage"
                    map_clean = map_raw.replace("de_", "").capitalize() if map_raw else None
                    map_name  = map_clean if map_clean in ACTIVE_MAPS else None
                    results.append({
                        "team1":  t1,
                        "team2":  t2,
                        "winner": g_winner,
                        "map":    map_name,
                        "format": "map",
                        "score1": 1 if g_winner == t1 else 0,
                        "score2": 1 if g_winner == t2 else 0,
                        "date":   g.get("begin_at"),
                        "stars":  0,
                    })
            else:
                # Fall back to series-level result if games list is empty
                score_data = {r["team_id"]: r["score"] for r in (m.get("results") or [])}
                s1 = score_data.get(t1_id, 0)
                s2 = next((v for k, v in score_data.items() if k != t1_id), 0)
                results.append({
                    "team1":  t1,
                    "team2":  t2,
                    "winner": winner,
                    "map":    None,
                    "format": "series",
                    "score1": s1,
                    "score2": s2,
                    "date":   m.get("begin_at"),
                    "stars":  0,
                })

        page += 1

    print(f"  PandaScore: {len(results)} map/series results from {total_series} matched series "
          f"({skipped} skipped — team names not in our set).")
    return results


# ── Public API ────────────────────────────────────────────────────────────────

def fetch_and_cache(count: int = 30) -> None:
    """Run the fetch pipeline and write data/cache/*.json files.

    Args:
        count: Number of top-ranked teams to fetch and cache (default 30).
               Use 50 or 100 for larger tournament pools.  The synthetic
               fallback generates teams for any ranks beyond the built-in 30.

    Source priority:
      0. gigobyte/HLTV (Node.js) — rankings + rosters + real match results
      1. hltvorg-api (Selenium)  — rankings + rosters only
      2. Plain HTTP scrape       — rankings only (often blocked)
      3. Built-in synthetic data — always works, no network needed
    """
    print(f"Fetching HLTV data (top {count} teams)…")

    live_results: list = []

    # ── Priority 0: gigobyte/HLTV ────────────────────────────────────────────
    gigobyte = _try_gigobyte_hltv(count=count)
    if gigobyte:
        live_teams, live_results = gigobyte
        source_label = (
            f"gigobyte/HLTV — {len(live_teams)} teams, {len(live_results)} results"
        )
    else:
        # ── Priority 1: hltvorg-api (Selenium) ───────────────────────────────
        live_teams = _try_hltvorg_api()
        if live_teams:
            n_rosters = sum(1 for t in live_teams if t.get("players"))
            source_label = f"hltvorg-api — {len(live_teams)} teams ({n_rosters} with rosters)"
        else:
            # ── Priority 2: plain HTTP scrape ─────────────────────────────────
            live_teams = _try_requests_scrape()
            source_label = (
                f"HTTP scrape — {len(live_teams)} teams (names/rankings only)"
                if live_teams else None
            )

    is_synthetic = live_teams is None
    if is_synthetic:
        print("  All live sources failed — using built-in synthetic data.")
    else:
        print(f"  Live data source: {source_label}")

    # Build base data from synthetic template, then overwrite with live names/rosters
    teams, players = _build_synthetic_data(count=count)
    if live_teams:
        _apply_live_data(teams, players, live_teams)

    # Load and apply manual overrides (always, even on pure-synthetic runs)
    overrides = _load_overrides()
    _apply_overrides(teams, players, overrides)

    # ── Deduplicate player names across teams ─────────────────────────────────
    # Run AFTER overrides so override rosters are treated as authoritative.
    # Keeps the player on the better-ranked team; replaces stale duplicates on
    # weaker teams with a placeholder so every team retains 5 players.
    if live_teams:
        _dedup_players(teams, players)

    # ── PandaScore fallback when HLTV results were blocked ───────────────────
    # Only runs when the Node.js HLTV fetch delivered 0 match results.
    # Requires a free PandaScore token — see pandascore.co.
    if not live_results:
        ps_token = _load_pandascore_token()
        if ps_token:
            team_name_set = {t["name"] for t in teams}
            ps_results = _try_pandascore_results(team_name_set, ps_token, months=3)
            if ps_results:
                live_results = ps_results
                print(f"  Using PandaScore as results source ({len(live_results)} results).")
            else:
                print("  PandaScore returned 0 usable results — training on synthetic matches only.")
        else:
            print(
                "  ⚠  No PandaScore token — HLTV results are Cloudflare-blocked and no fallback "
                "is available.\n"
                "     To enable real CS2 match data:\n"
                "       1. Register free at https://pandascore.co/\n"
                "       2. Copy your token from My Account → API Access Token\n"
                f"       3. Save it to: {_PANDASCORE_TOKEN_FILE}\n"
                "     Training will use synthetic matches only until then."
            )

    # Merge any manual match results from overrides into the live results list
    for m in overrides.get("results", {}).get("matches", []):
        live_results.append({
            "team1":  m["team_a"],
            "team2":  m["team_b"],
            "winner": m["winner"],
            "score1": 1, "score2": 0,
            "map":    m.get("map"),
            "format": None, "date": None, "stars": 0,
        })

    # Use real/override match results when available, pad with synthetic if needed
    matches = (
        _build_matches_from_results(live_results, teams)
        if live_results
        else _build_synthetic_matches(teams)
    )

    # Strip the internal _rank join-key before writing to disk
    for p in players:
        p.pop("_rank", None)

    cache_write("teams",   {"teams": teams, "synthetic": is_synthetic,
                            "active_maps": list(ACTIVE_MAPS)})
    cache_write("players", {"players": players})
    cache_write("matches", {"matches": matches})

    print(f"  Cached {len(teams)} teams, {len(players)} players, {len(matches)} matches.")
