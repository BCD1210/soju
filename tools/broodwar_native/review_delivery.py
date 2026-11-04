# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow", "scikit-image"]
# ///
# Run via scripts/broodwar-native.py assets-review <delivery>.
"""Delivery review process entry point in the private asset environment."""
import pathlib
import sys
from assets_review import review

if __name__ == '__main__':
    raise SystemExit(0 if review(pathlib.Path(sys.argv[1])).passed else 1)
