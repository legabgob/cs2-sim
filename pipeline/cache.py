"""Cache management: read/write JSON under data/cache/ with timestamp tracking."""
import json
import os
import time
from datetime import datetime
from pathlib import Path

CACHE_DIR = Path(__file__).parent.parent / "data" / "cache"
STALE_DAYS = 14

CACHE_DIR.mkdir(parents=True, exist_ok=True)


def _meta_path() -> Path:
    return CACHE_DIR / "metadata.json"


def _load_meta() -> dict:
    p = _meta_path()
    if p.exists():
        with open(p) as f:
            return json.load(f)
    return {}


def _save_meta(meta: dict) -> None:
    with open(_meta_path(), "w") as f:
        json.dump(meta, f, indent=2)


def cache_write(name: str, data: object) -> None:
    path = CACHE_DIR / f"{name}.json"
    with open(path, "w") as f:
        json.dump(data, f, indent=2)
    meta = _load_meta()
    meta[name] = {"updated_at": time.time(), "date": datetime.utcnow().isoformat()}
    _save_meta(meta)


def cache_read(name: str) -> object | None:
    path = CACHE_DIR / f"{name}.json"
    if not path.exists():
        return None
    with open(path) as f:
        return json.load(f)


def cache_age_days(name: str) -> float:
    """Returns age in days, or inf if the cache entry doesn't exist."""
    meta = _load_meta()
    if name not in meta:
        return float("inf")
    return (time.time() - meta[name]["updated_at"]) / 86400.0


def cache_is_stale(name: str) -> bool:
    return cache_age_days(name) > STALE_DAYS


def oldest_cache_age_days() -> float:
    """Age of the oldest required cache entry (teams/players/matches)."""
    return max(cache_age_days(k) for k in ("teams", "players", "matches"))
