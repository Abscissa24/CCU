#!/usr/bin/env python3
from __future__ import annotations

import re
import time
import unicodedata
from datetime import date, datetime
from pathlib import Path

import questionary
import requests
from bs4 import BeautifulSoup
from rich.console import Console
from rich.panel import Panel
from rich.table import Table

console = Console()

SEARCH_URL = "https://ratings.chessa.co.za/playersearch.php"
SEARCH_FIELD = "speler"            # name of the search box's form field (from the site's HTML)
PROFILE_URL = "https://ratings.chessa.co.za/playerpage.php?cid={}"
REQUEST_HEADERS = {"User-Agent": "chessa-rating-finder/1.0 (personal rating lookup tool)"}
TIMEOUT_SECONDS = 15
MIN_SECONDS_BETWEEN_REQUESTS = 2.0
# If the site's page looks different from what we expect, the raw page is saved
# here so the problem can be investigated.
DEBUG_FILE = Path(__file__).resolve().parent / "last_response.html"

_last_request_time = 0.0


class SiteError(Exception):
    """A problem with the website (offline, slow, or an unexpected page).
    The message is written for the user, so it can be printed as-is."""


# --------------------------------------------------------------------------
# Small helpers
# --------------------------------------------------------------------------
def normalise(text: str) -> str:
    """Make names comparable: lower case, no accents, no punctuation noise.

    'Hendré' -> 'hendre', "O'Brien" -> 'obrien', 'Van-Der  Merwe' -> 'van der merwe'
    """
    text = unicodedata.normalize("NFKD", text or "")
    text = "".join(ch for ch in text if not unicodedata.combining(ch))
    text = text.casefold().replace("'", "").replace("\u2019", "").replace("-", " ")
    return " ".join(text.split())


def parse_input_date(text: str) -> date | None:
    """Turn 'YYYY-MM-DD' typed by the user into a date. Blank -> None.
    Raises ValueError if the text is not a valid date."""
    text = (text or "").strip()
    if not text:
        return None
    return datetime.strptime(text, "%Y-%m-%d").date()


def header_key(text: str) -> str:
    """'Chess SA id' -> 'chesssaid', 'LastActive' -> 'lastactive' (to match table headers)."""
    return re.sub(r"[^a-z0-9]", "", text.casefold())


# --------------------------------------------------------------------------
# Talking to the website
# --------------------------------------------------------------------------
def fetch_search_page(session: requests.Session, surname: str) -> str:
    """Send the surname to the site's search form and return the page's HTML."""
    global _last_request_time
    wait = MIN_SECONDS_BETWEEN_REQUESTS - (time.monotonic() - _last_request_time)
    if wait > 0:
        time.sleep(wait)                      # polite pause between requests
    _last_request_time = time.monotonic()
    try:
        response = session.post(SEARCH_URL, data={SEARCH_FIELD: surname},
                                headers=REQUEST_HEADERS, timeout=TIMEOUT_SECONDS)
        response.raise_for_status()
    except requests.Timeout:
        raise SiteError("The website took too long to respond. Please try again in a moment.")
    except requests.ConnectionError:
        raise SiteError("Could not connect to ratings.chessa.co.za. Check your internet connection "
                        "or whether the website is down.")
    except requests.HTTPError as exc:
        raise SiteError(f"The website returned an error (HTTP {exc.response.status_code}). Please try again later.")
    except requests.RequestException as exc:
        raise SiteError(f"The request failed: {exc}")
    if "charset" not in response.headers.get("Content-Type", "").lower():
        response.encoding = "utf-8"           # avoid garbled names like 'HendrÃ©'
    return response.text


def parse_name(cell_text: str) -> tuple[str, str, str]:
    """'Pillay, A (Armiel)' -> ('Pillay', 'A', 'Armiel').

    The first name is blank if the site shows no brackets."""
    match = re.match(r"^\s*([^,]+?)\s*,\s*([^()]*?)\s*(?:\(([^)]*)\))?\s*$", cell_text or "")
    if not match:
        return (cell_text or "").strip(), "", ""
    surname, initials, first = match.groups()
    return surname.strip(), initials.strip(), (first or "").strip()


def parse_rating_cell(text: str) -> str | None:
    """Return the rating exactly as the site shows it, or None if missing.
    A real '0' stays '0' - it is NOT treated as missing."""
    text = (text or "").strip()
    if not text:
        return None
    try:
        float(text)
    except ValueError:
        return None
    return text


def parse_search_results(html: str) -> list[dict]:
    """Read the player rows out of the search-results page.

    Columns are found by their HEADER TEXT, not by position, so a re-ordered
    table still works. Raises SiteError if the page is not what we expect.
    """
    soup = BeautifulSoup(html, "html.parser")

    table, headers, labels = None, [], []
    for candidate in soup.find_all("table"):
        first_row = candidate.find("tr")
        if first_row is None:
            continue
        cells = first_row.find_all(["th", "td"])
        keys = [header_key(c.get_text()) for c in cells]
        if {"chesssaid", "name", "dob"} <= set(keys):
            table, headers = candidate, keys
            labels = [c.get_text(strip=True) for c in cells]
            break

    if table is None:
        # A results page with zero players may have no table at all.
        if "search results for" in soup.get_text(" ").casefold():
            return []
        raise SiteError("The website returned a page I don't recognise (its layout may have changed).")

    # Rating columns are the ones between 'Registered' and 'LastActive'
    # (Main, Standard, Rapid, Blitz at the time of writing).
    if "registered" in headers and "lastactive" in headers:
        rating_cols = list(range(headers.index("registered") + 1, headers.index("lastactive")))
    else:
        rating_cols = [i for i, key in enumerate(headers) if key in ("main", "standard", "rapid", "blitz")]
    if not rating_cols:
        raise SiteError("The results table has no rating columns I recognise (its layout may have changed).")

    def cell(cells: list[str], key: str) -> str:
        return cells[headers.index(key)] if key in headers else ""

    players = []
    for row in table.find_all("tr")[1:]:               # [1:] skips the header row
        cells = [td.get_text(strip=True) for td in row.find_all("td")]
        if len(cells) < len(headers):
            continue                                    # spacer/odd row
        surname, initials, first_name = parse_name(cell(cells, "name"))
        birth_date = None
        for fmt in ("%Y-%m-%d", "%Y/%m/%d"):
            try:
                birth_date = datetime.strptime(cell(cells, "dob"), fmt).date()
                break
            except ValueError:
                pass
        players.append({
            "id": cell(cells, "chesssaid"),
            "surname": surname, "initials": initials, "first_name": first_name,
            "birth_date": birth_date,
            "gender": cell(cells, "gender"), "age_group": cell(cells, "agegroup"),
            "region": cell(cells, "region"), "registered": cell(cells, "registered"),
            "last_active": cell(cells, "lastactive"),
            "ratings": {labels[i]: parse_rating_cell(cells[i]) for i in rating_cols},
        })
    return players


# --------------------------------------------------------------------------
# Searching and matching
# --------------------------------------------------------------------------
def search_players_by_surname(session: requests.Session, surname: str) -> tuple[list[dict], int]:
    """search the site, return (exact surname matches, total rows the site returned).

    The site may also return partial matches, so we keep only rows whose
    surname equals the typed one (ignoring case, accents and hyphens)."""
    html = fetch_search_page(session, surname)
    try:
        rows = parse_search_results(html)
    except SiteError as exc:
        try:
            DEBUG_FILE.write_text(html, encoding="utf-8")
            raise SiteError(f"{exc} The page was saved as {DEBUG_FILE.name} for investigation.")
        except OSError:
            raise exc
    exact = [p for p in rows if normalise(p["surname"]) == normalise(surname)]
    exact.sort(key=lambda p: (normalise(p["first_name"]), p["id"]))
    return exact, len(rows)


def first_names_match(record_first: str, typed_first: str) -> bool | None:
    """True/False if the site shows a first name; None if it doesn't.

    True if every word the user typed appears in the player's first names:
    'Hendre' matches 'Hendré Pieter'; 'John' does NOT match 'Johnny'."""
    record = normalise(record_first).split()
    if not record:
        return None
    typed = normalise(typed_first).split()
    return bool(typed) and all(word in record for word in typed)


def verify_date_of_birth(record_dob: date | None, typed_dob: date | None) -> bool | None:
    """True = same date, False = different date,
    None = cannot tell (the site or the user gave no date)."""
    if record_dob is None or typed_dob is None:
        return None
    return record_dob == typed_dob


def match_player_details(candidates: list[dict], first_name: str, dob: date | None):
    """narrow the surname matches using first name and date of birth.

    Returns (status, matches) where each match is (player, first_name_ok, dob_ok):
      'verified' - first name AND date of birth both match
      'partial'  - only one of them matches (or one could not be checked);
                   the user must choose, and the result is labelled unverified
      'none'     - nothing matches; no ratings are shown
    """
    scored = [(p, first_names_match(p["first_name"], first_name), verify_date_of_birth(p["birth_date"], dob))
              for p in candidates]
    verified = [s for s in scored if s[1] is True and s[2] is True]
    if verified:
        return "verified", verified
    partial = [s for s in scored if s[1] is True or s[2] is True]
    if partial:
        return "partial", partial
    return "none", []


def describe_verification(first_ok: bool | None, dob_ok: bool | None) -> str:
    """Plain-English identity status shown with the result."""
    if first_ok is True and dob_ok is True:
        return "Verified (first name and date of birth match)"
    notes = []
    if first_ok is False:
        notes.append("first name does not match")
    elif first_ok is None:
        notes.append("first name not shown by the site")
    if dob_ok is False:
        notes.append("date of birth does NOT match")
    elif dob_ok is None:
        notes.append("date of birth could not be checked (not provided or not shown by the site)")
    return "NOT fully verified: " + "; ".join(notes)


# --------------------------------------------------------------------------
# Ratings
# --------------------------------------------------------------------------
def extract_player_ratings(player: dict) -> list[tuple[str, str | None]]:
    """(category, rating) for every rating column the site shows.
    None means the site showed nothing; a real '0' stays '0'."""
    return list(player["ratings"].items())


# --------------------------------------------------------------------------
# Terminal screens
# --------------------------------------------------------------------------
def display_header() -> None:
    console.print(Panel("[bold]CHESSA RATING FINDER[/bold]\nCHESS SOUTH AFRICA RATINGS",
                        expand=False, padding=(0, 6), border_style="cyan"))


def get_player_details():
    """Ask for surname, first name and date of birth. Returns None if cancelled."""
    console.print("\n[bold]PLAYER INFORMATION[/bold]")

    def valid_date(text: str):
        try:
            parse_input_date(text)
            return True
        except ValueError:
            return "Use YYYY-MM-DD, e.g. 2004-04-08 (or leave blank)"

    surname = questionary.text("Surname:", validate=lambda t: bool(t.strip()) or "Surname is required").ask()
    if surname is None:
        return None
    first = questionary.text("First Name:", validate=lambda t: bool(t.strip()) or "First name is required").ask()
    if first is None:
        return None
    dob_text = questionary.text("Date of Birth (YYYY-MM-DD, blank if unknown):", validate=valid_date).ask()
    if dob_text is None:
        return None
    return surname.strip(), first.strip(), parse_input_date(dob_text)


def _rating_summary(player: dict) -> str:
    parts = [f"{cat} {value}" for cat, value in player["ratings"].items() if value is not None]
    return ", ".join(parts) if parts else "no ratings"


def choose_player(matches):
    """Scenario C: show the possible players and let the user pick (never auto-pick).

    Only the BIRTH YEAR is listed (not the full date) - enough to tell people
    apart without displaying other players' full birth dates."""
    choices = []
    for player, first_ok, dob_ok in matches:
        year = player["birth_date"].year if player["birth_date"] else "unknown"
        reasons = []
        if first_ok is False:
            reasons.append("first name differs")
        elif first_ok is None:
            reasons.append("first name unknown")
        if dob_ok is False:
            reasons.append("DOB differs")
        elif dob_ok is None:
            reasons.append("DOB unchecked")
        note = f"  [{'; '.join(reasons)}]" if reasons else ""
        name = f"{player['surname']}, {player['first_name'] or player['initials']}"
        title = f"{name} | born {year} | {player['region'] or '-'} | ID {player['id']} | {_rating_summary(player)}{note}"
        choices.append(questionary.Choice(title, value=(player, first_ok, dob_ok)))
    choices.append(questionary.Choice("None of these - cancel", value=None))
    return questionary.select("Several possible players. Which one is correct?", choices=choices).ask()


def display_player_results(player: dict, verification: str) -> None:
    """Print the player details and the ratings table."""
    console.print(Panel("[bold green]PLAYER FOUND[/bold green]", expand=False, padding=(0, 14)))
    console.print("[bold]PLAYER DETAILS[/bold]")
    console.print(f"Surname: {player['surname']}")
    console.print(f"First Name: {player['first_name'] or 'not shown by the site'}")
    console.print(f"Region: {player['region'] or 'not shown'}")
    colour = "green" if verification.startswith("Verified") else "yellow"
    console.print(f"Identity check: [{colour}]{verification}[/{colour}]")

    table = Table(title="CHESS RATINGS")
    table.add_column("Rating Category")
    table.add_column("Rating", justify="right")
    for category, rating in extract_player_ratings(player):
        table.add_row(category, rating if rating is not None else "[dim]Not available[/dim]")
    console.print(table)

    console.print(f"Last active (as shown by the site): {player['last_active'] or 'not shown'}")
    console.print(f"Retrieved: {datetime.now():%Y-%m-%d %H:%M}")
    console.print("Source: ratings.chessa.co.za (live search)")
    console.print(f"Profile: {PROFILE_URL.format(player['id'])}\n")


def run_search(session: requests.Session) -> None:
    """One complete search: ask -> search -> match -> (choose) -> display."""
    details = get_player_details()
    if details is None:
        return
    surname, first_name, dob = details

    try:
        with console.status("Searching ratings.chessa.co.za..."):
            candidates, total_rows = search_players_by_surname(session, surname)
    except SiteError as exc:
        console.print(f"[red]{exc}[/red]\n")
        return

    if not candidates:                                   # Scenario E
        extra = f" ({total_rows} other player(s) matched only part of it.)" if total_rows else ""
        console.print(f"[red]No player with surname '{surname}' was found.[/red]{extra}\n")
        return

    status, matches = match_player_details(candidates, first_name, dob)
    if status == "none":                                 # surname only: never show ratings
        console.print(f"[red]No matching player found.[/red] {len(candidates)} player(s) have the surname "
                      f"'{surname}', but none match the first name or date of birth you gave.\n")
        return

    if status == "verified" and len(matches) == 1:       # Scenario A
        chosen = matches[0]
    else:                                                # Scenarios B/C/D
        if status == "partial":
            console.print("[yellow]No player matched both first name and date of birth.[/yellow]")
        chosen = choose_player(matches)
        if chosen is None:
            console.print("Cancelled.\n")
            return

    player, first_ok, dob_ok = chosen
    display_player_results(player, describe_verification(first_ok, dob_ok))


def main() -> None:
    display_header()
    session = requests.Session()
    try:
        while True:
            action = questionary.select("What would you like to do?", choices=[
                questionary.Choice("Search for a player", value="search"),
                questionary.Choice("Exit", value="exit"),
            ]).ask()
            if action in (None, "exit"):
                break
            run_search(session)
    except KeyboardInterrupt:
        pass
    console.print("Goodbye!")


if __name__ == "__main__":
    main()
