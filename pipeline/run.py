#!/usr/bin/env python3
"""CS2 Tournament Simulator — data pipeline CLI.

Commands:
  fetch          Scrape/cache HLTV data and write data/cache/*.json
  train          Train PyTorch models and export ONNX to data/
  fetch+train    Both in sequence

Options:
  --count N      Number of top-ranked teams to fetch (default: 30, e.g. 50 or 100).
                 Ranks beyond the built-in 30 use generated synthetic stats as a
                 fallback when live data is unavailable.

Examples:
  python pipeline/run.py fetch+train
  python pipeline/run.py fetch+train --count 50
  python pipeline/run.py fetch --count 100
  python pipeline/run.py train          # retrain only, no network call
"""
import sys
import argparse
import logging
from pathlib import Path

# Ensure the project root is on sys.path so `import pipeline.*` works when the
# script is invoked directly (i.e. not as a module via -m).
_project_root = str(Path(__file__).resolve().parent.parent)
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s  %(levelname)-8s  %(message)s",
    datefmt="%H:%M:%S",
)

VALID_COMMANDS = ("fetch", "train", "fetch+train")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python pipeline/run.py",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "command",
        choices=VALID_COMMANDS,
        metavar="command",
        help=f"One of: {', '.join(VALID_COMMANDS)}",
    )
    parser.add_argument(
        "--count",
        type=int,
        default=30,
        metavar="N",
        help="Top-N teams to fetch (default: 30)",
    )
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()

    if args.command in ("fetch", "fetch+train"):
        from pipeline.scraper import fetch_and_cache
        fetch_and_cache(count=args.count)

    if args.command in ("train", "fetch+train"):
        from pipeline.train import train_and_export
        train_and_export()


if __name__ == "__main__":
    main()
