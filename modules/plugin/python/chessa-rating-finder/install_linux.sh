#!/usr/bin/env bash
cd "$(dirname "$0")"

command -v python3 >/dev/null 2>&1 || {
    echo "Python 3.10+ is required."
    exit 1
}

python3 -c 'import sys; exit(sys.version_info < (3,10))' || {
    echo "Python 3.10 or newer is required."
    exit 1
}

if [[ ! -x ".venv/bin/python" ]]; then
    python3 -m venv .venv || exit 1
fi

.venv/bin/python -m pip install -r requirements.txt || exit 1

clear 2>/dev/null || true
exec .venv/bin/python chessa_rating_tui.py
