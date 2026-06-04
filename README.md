# CS2 Tournament Simulator

A desktop application that simulates CS2 professional tournaments using a Monte Carlo engine backed by real team and player data from HLTV. It predicts win probabilities, stage-reach rates, and individual player performance ratings across thousands of simulated runs.

![GUI overview — four-panel layout: Setup · Tournament Builder · Roster · Results](docs/screenshot.png)

---

## How it works

### Architecture

The project is split into two halves that communicate through a JSON data cache:

```
pipeline/          ← Python — fetches data, trains ML models, writes cache
  scraper.py       ← HLTV + PandaScore data fetching
  features.py      ← Feature engineering for training
  train.py         ← PyTorch MLP training + ONNX export
  run.py           ← CLI entry point

src/               ← Rust — loads cache, runs simulations, renders GUI
  simulation.rs    ← Monte Carlo engine (parallel runs via Rayon)
  veto.rs          ← Map veto simulation (softmax ban/pick selection)
  inference.rs     ← ONNX inference via ort (match + player models)
  ui/              ← egui panels: Setup, Tournament Builder, Roster, Results
  data.rs          ← Cache loader
```

### Data pipeline

Running `python pipeline/run.py fetch+train` does three things:

1. **Fetch** — Scrapes HLTV via the [gigobyte/HLTV](https://github.com/gigobyte/HLTV) Node.js library (rankings, rosters). Falls back to PandaScore for match results when HLTV's `/results` endpoint is Cloudflare-blocked.

2. **Train** — Builds two PyTorch MLPs:
   - **MatchNet** — binary classifier: given team A vs team B stats on a map, who wins? (trained on real + synthetic match results)
   - **PlayerNet** — regression: predict a player's in-game rating given their stats and the opponent's strength.
   
   Both models are exported to ONNX for the Rust runtime.

3. **Cache** — Writes `data/cache/teams.json`, `players.json`, `matches.json` and the ONNX models + `feature_schema.json`.

### Monte Carlo simulation

Each simulation run:
1. Clones the selected teams into the tournament bracket.
2. Iterates through stages (Swiss, Single/Double Elimination, GSL, Round Robin).
3. For each match: simulates a map veto (softmax-weighted ban/pick based on team map win rates), then predicts the winner of each map using MatchNet.
4. Tracks which teams advance and which players collected top ratings.

Running 10 000 iterations (default) takes ~1–2 s on a modern CPU thanks to Rayon parallelism.

### Map veto model

Each team has a per-map win rate. Bans are weighted toward worst maps (softmax with temperature 4), picks toward best maps. The result realistically reflects actual team preferences — teams that dominate on a map will pick it; teams that struggle will ban it first.

### Stand-in support

In the **Roster** panel you can replace any player slot with a stand-in for the duration of a session: enter the stand-in's name and rating. The substitution propagates into both the match prediction (via `team_avg_rating`) and the player performance display — without touching the on-disk cache.

---

## Requirements

| Tool | Version | Purpose |
|---|---|---|
| **Rust** | ≥ 1.75 | GUI + simulation engine |
| **Python** | ≥ 3.10 | Data pipeline |
| **Node.js** | ≥ 18 | HLTV data fetching |
| **uv** (optional) | any | Fast Python env management |

---

## Installation

### 1. Clone

```bash
git clone https://github.com/YOUR_USERNAME/cs2-sim.git
cd cs2-sim
```

### 2. Python environment

```bash
# With uv (recommended):
uv venv && uv pip install -r requirements.txt

# Or with plain pip:
python -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
```

### 3. Node.js dependencies

```bash
cd pipeline/hltv-node
npm install
cd ../..
```

### 4. Run the data pipeline

```bash
# Activate the Python env first if not using uv
source .venv/bin/activate   # or: .venv/bin/python ...

python pipeline/run.py fetch+train
```

This fetches the current HLTV top-30 teams, trains the models, and writes `data/cache/` and the ONNX files. It takes ~3–5 minutes (most of the time is courtesy delays to avoid Cloudflare rate-limiting).

> **Note:** HLTV's `/results` and `/stats/matches` endpoints are aggressively Cloudflare-protected. Match results automatically fall back to **PandaScore** (free tier). See [Enabling real match results](#enabling-real-match-results) below.

### 5. Build and run the GUI

```bash
cargo run --release
```

On the first run Rust downloads and compiles all dependencies (~2 minutes). Subsequent builds are incremental.

---

## Usage

### Workflow

```
fetch+train  →  open GUI  →  configure tournament  →  run simulation  →  view results
```

### Pipeline CLI

```bash
# Fetch data + train models (most common)
python pipeline/run.py fetch+train

# Fetch top 50 teams instead of 30
python pipeline/run.py fetch+train --count 50

# Re-train models without re-fetching (fast, uses cached data)
python pipeline/run.py train

# Fetch only
python pipeline/run.py fetch
```

### GUI panels

**Setup** (left panel)
- Select teams for the tournament (up to any number).
- Toggle map pool — permaban maps that aren't in rotation.
- Adjust recency bias: "Historical" weights all data equally; "Recent" up-weights the last month's form.
- Set number of Monte Carlo runs (1 000 → 100 000).

**Tournament Builder** (second panel)
- Choose a preset or build a custom format from scratch.
- Built-in presets: **CS2 Major** (Swiss + single-elim playoffs), **ESL Pro League Season** (GSL groups + single-elim), **IEM Single Site** (round-robin + single-elim).
- Stages can be Swiss, GSL, Single Elimination, Double Elimination, or Round Robin.
- Save/load custom formats as JSON.

**Roster** (third panel)
- Per-player form sliders (−0.30 → +0.30) applied on top of base ratings.
- **↔ Sub** button on each player row: replace with a stand-in for this session. Enter the stand-in's name and rating; the change flows into both win-probability and player-performance predictions. Click **✕ Restore** to revert.
- **Reset all** clears all form adjustments and stand-ins.

**Results** (main panel)
Three tabs after simulation completes:
- **Win Probabilities** — bar chart of each team's championship probability.
- **Stage Reach** — stacked probability of reaching each stage.
- **Player Performance** — predicted average rating ± std dev for every player across all runs.

### ✏ Edit Data

The floating **Edit Data** editor (toolbar button) lets you manually override team stats, player rosters, and add training results — all saved to `data/overrides.json` and applied on the next `fetch+train` run.

---

## Enabling real match results

HLTV's results endpoint is Cloudflare-blocked. Without real data, the model trains on synthetic matches and achieves ~0.72 validation accuracy — functional but less realistic.

**PandaScore** (free tier, ~100 req/hour) provides real CS2 match data:

1. Register at [pandascore.co](https://pandascore.co/)
2. Go to **My Account → API Access Token** and copy your token
3. Save it:
   ```bash
   echo "YOUR_TOKEN_HERE" > data/pandascore_token.txt
   ```
4. Re-run the pipeline:
   ```bash
   python pipeline/run.py fetch+train
   ```

With real results the model captures genuine upset rates and team dynamics, producing more varied and realistic simulations.

---

## Updating the map pool

The active map pool is defined in **one place** and flows through the entire stack automatically:

```python
# pipeline/scraper.py  ← edit this when Valve rotates a map
ACTIVE_MAPS = ["Dust2", "Mirage", "Inferno", "Nuke", "Ancient", "Anubis", "Train"]
```

After changing it, run `python pipeline/run.py fetch+train`. The Rust app reads the pool from `data/cache/teams.json` on startup — no recompile needed.

---

## Project structure

```
cs2-sim/
├── pipeline/               Python data pipeline
│   ├── run.py              CLI: fetch / train / fetch+train  --count N
│   ├── scraper.py          HLTV + PandaScore scraping, synthetic fallback
│   ├── features.py         Feature vectors for training
│   ├── train.py            PyTorch training + ONNX export
│   ├── cache.py            Cache read/write helpers
│   └── hltv-node/          Node.js HLTV wrapper
│       ├── fetch.mjs       gigobyte/HLTV API client
│       └── package.json
│
├── src/                    Rust GUI + simulation
│   ├── main.rs             App entry point (eframe)
│   ├── app.rs              Central app state
│   ├── simulation.rs       Monte Carlo engine (Rayon parallel)
│   ├── veto.rs             Map veto simulation
│   ├── inference.rs        ONNX inference (ort)
│   ├── data.rs             Cache loader
│   ├── types.rs            Team / Player / Substitute types
│   ├── overrides.rs        data/overrides.json serde
│   ├── presets.rs          Built-in tournament formats
│   └── ui/
│       ├── mod.rs          Panel layout
│       ├── setup.rs        Setup panel
│       ├── builder.rs      Tournament builder panel
│       ├── players.rs      Roster + stand-in panel
│       ├── results.rs      Results panel
│       └── editor.rs       Floating data editor
│
├── data/
│   ├── overrides.json      Manual team/player/results overrides
│   ├── cache/              Generated by pipeline (gitignored)
│   │   ├── teams.json
│   │   ├── players.json
│   │   └── matches.json
│   ├── model.onnx          Match prediction model (gitignored)
│   └── player_model.onnx   Player rating model (gitignored)
│
├── Cargo.toml
└── requirements.txt
```

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `teams.json not found` | Run `python pipeline/run.py fetch+train` first |
| ONNX models not found | Run `python pipeline/run.py train` |
| All teams show 0.0% win probability | Cached data is empty — re-run the pipeline |
| Stale data warning (orange banner) | Re-run `fetch+train`; data is > 14 days old |
| Synthetic data notice (blue banner) | HLTV was unreachable; teams are from built-in data. Results are still usable. |
| CF blocks on every roster | Normal — HLTV rate-limits heavily. Rosters still succeed after retries. |
| 0 match results from HLTV | Also normal. Add a PandaScore token for real match data. |
| `node not found` | Install Node.js ≥ 18 |
| `npm install` fails | Check Node version: `node --version` |

---

## Dependencies

### Rust
- [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) 0.28 — immediate-mode GUI
- [ort](https://github.com/pykeio/ort) 2.0 rc — ONNX Runtime bindings
- [rayon](https://github.com/rayon-rs/rayon) — data-parallel Monte Carlo runs
- [rand](https://github.com/rust-random/rand) — SmallRng for deterministic seeds

### Python
- [PyTorch](https://pytorch.org/) — model training
- [onnx](https://github.com/onnx/onnx) + [onnxscript](https://github.com/microsoft/onnxscript) — ONNX export
- [scikit-learn](https://scikit-learn.org/) — feature scaling, train/test split

### Node.js
- [gigobyte/HLTV](https://github.com/gigobyte/HLTV) — unofficial HLTV API client

---

## License

MIT
