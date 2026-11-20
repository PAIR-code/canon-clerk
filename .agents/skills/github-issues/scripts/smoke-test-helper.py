#!/usr/bin/env python3
"""Helper script for smoke-testing agent script canons."""

import json
import urllib.request


def main() -> None:
    print(json.dumps({"status": "ok"}))


if __name__ == "__main__":
    main()
