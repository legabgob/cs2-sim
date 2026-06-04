"""Feature engineering: build training arrays from cached data."""
import numpy as np
from .cache import cache_read

MATCH_FEATURE_NAMES = [
    "team_a_avg_rating",
    "team_b_avg_rating",
    "team_a_recent_wr",
    "team_b_recent_wr",
    "h2h_wr",
    "team_a_map_wr",
    "team_b_map_wr",
    "map_in_a_pool",
    "map_in_b_pool",
    "recency_weight",
]
N_MATCH_FEATURES = len(MATCH_FEATURE_NAMES)

PLAYER_FEATURE_NAMES = [
    "avg_rating",
    "kd_ratio",
    "adr",
    "kast",
    "opening_duel_success",
    "opponent_avg_rating",
    "map_familiarity",
    "form_modifier",
    "recency_weight",
]
N_PLAYER_FEATURES = len(PLAYER_FEATURE_NAMES)


def build_match_training_data(recency_weight: float = 0.5) -> tuple[np.ndarray, np.ndarray]:
    """Returns (X, y) arrays for match outcome prediction."""
    raw = cache_read("matches")
    if raw is None:
        raise RuntimeError("No match cache found. Run 'fetch' first.")
    matches = raw["matches"]

    X, y = [], []
    for m in matches:
        feat = [
            m["team_a_rating"],
            m["team_b_rating"],
            m["team_a_recent_wr"],
            m["team_b_recent_wr"],
            m["h2h_wr"],
            m["team_a_map_wr"],
            m["team_b_map_wr"],
            m["map_in_a_pool"],
            m["map_in_b_pool"],
            recency_weight,
        ]
        X.append(feat)
        y.append(1.0 if m["winner"] == m["team_a"] else 0.0)

    # Augment: flip team_a / team_b to double data and add symmetry
    X_aug, y_aug = [], []
    for feat, label in zip(X, y):
        X_aug.append(feat)
        y_aug.append(label)
        flipped = [
            feat[1], feat[0],   # swap ratings
            feat[3], feat[2],   # swap win rates
            1.0 - feat[4],      # flip h2h
            feat[6], feat[5],   # swap map win rates
            feat[8], feat[7],   # swap pool flags
            feat[9],            # recency stays
        ]
        X_aug.append(flipped)
        y_aug.append(1.0 - label)

    return np.array(X_aug, dtype=np.float32), np.array(y_aug, dtype=np.float32)


def build_player_training_data() -> tuple[np.ndarray, np.ndarray]:
    """Returns (X, y) for player rating prediction."""
    import random
    rng = random.Random(7)

    raw_p = cache_read("players")
    raw_t = cache_read("teams")
    if raw_p is None or raw_t is None:
        raise RuntimeError("No player/team cache found. Run 'fetch' first.")

    players = raw_p["players"]
    teams = {t["name"]: t for t in raw_t["teams"]}
    all_maps = list(next(iter(players))["map_ratings"].keys())

    X, y = [], []
    for player in players:
        team = teams.get(player["team"], {})
        opp_teams = list(teams.values())
        for map_name in all_maps:
            opp = rng.choice(opp_teams)
            map_fam = player["map_ratings"].get(map_name, player["rating"])
            recency_w = rng.uniform(0.0, 1.0)
            feat = [
                player["rating"],
                player["kd"],
                player["adr"],
                player["kast"],
                player["opening_duel_success"],
                opp.get("avg_rating", 1.0),
                map_fam,
                0.0,  # form_modifier = 0 during training
                recency_w,
            ]
            X.append(feat)
            # Target: player's map-specific rating with some noise
            target = map_fam + rng.gauss(0, 0.05)
            y.append(target)

    return np.array(X, dtype=np.float32), np.array(y, dtype=np.float32)
