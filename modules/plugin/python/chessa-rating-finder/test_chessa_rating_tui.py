import tempfile
import unittest
from datetime import date
from pathlib import Path
from unittest import mock

import requests

import chessa_rating_tui as t

HEAD = ("<tr><th>#</th><th>Chess SA id</th><th>Name</th><th>Gender</th><th>Dob</th><th>AgeGroup</th>"
        "<th>Region</th><th>Citizen</th><th>Registered</th><th>Main</th><th>Standard</th><th>Rapid</th>"
        "<th>Blitz</th><th>LastActive</th></tr>")


def row(n, cid, name, dob, main="1200", std="1200", rapid="1200", blitz="1200", last="2025-05-04"):
    return (f"<tr><td>{n}</td><td>{cid}</td><td><a href='x'>{name}</a></td><td>Male</td><td>{dob}</td>"
            f"<td>Adult</td><td>Ethekwini (KED)</td><td>RSA</td><td>Yes</td><td>{main}</td><td>{std}</td>"
            f"<td>{rapid}</td><td>{blitz}</td><td>{last}</td></tr>")


def page(*rows):
    return ("<html><body><h4>SEARCH PLAYER</h4><b>Search results for: \"x\"</b>"
            f"<table>{HEAD}{''.join(rows)}</table></body></html>")


class FakeResponse:
    def __init__(self, text, status=200, ctype="text/html; charset=UTF-8"):
        self.text, self.status_code, self.headers, self.encoding = text, status, {"Content-Type": ctype}, None

    def raise_for_status(self):
        if self.status_code >= 400:
            err = requests.HTTPError("boom")
            err.response = self
            raise err


class FakeSession:
    def __init__(self, response=None, error=None):
        self.response, self.error, self.calls = response, error, []

    def post(self, url, **kwargs):
        self.calls.append((url, kwargs))
        if self.error:
            raise self.error
        return self.response


SMITHS = page(
    row(1, "1001", "Smith, J (John)", "2010-05-31", main="1500.5"),
    row(2, "1002", "Smith, J (John)", "2012-01-02", main="1100"),
    row(3, "1003", "Smith, J (Jane)", "2010-05-31"),
    row(4, "1004", "Smith, S (Sam)", ""),
    row(5, "1005", "Smith-Jones, A (Anna)", "2000-01-01"),     # partial surname match
)


def lookup(html, surname, first, dob):
    session = FakeSession(FakeResponse(html))
    with mock.patch.object(t, "MIN_SECONDS_BETWEEN_REQUESTS", 0):
        found, total = t.search_players_by_surname(session, surname)
    return found, total, t.match_player_details(found, first, t.parse_input_date(dob))


class Tests(unittest.TestCase):
    def test1_surname_returns_correct_player(self):
        html = page(row(1, "1040319728", "Pillay, A (Armiel)", "2004-04-08", "1270.5", "1269.9", "1200", "1200"))
        found, total, (status, m) = lookup(html, "Pillay", "Armiel", "2004-04-08")
        self.assertEqual(status, "verified")
        p = m[0][0]
        self.assertEqual((p["id"], p["first_name"], p["initials"]), ("1040319728", "Armiel", "A"))
        self.assertEqual(t.extract_player_ratings(p),
                         [("Main", "1270.5"), ("Standard", "1269.9"), ("Rapid", "1200"), ("Blitz", "1200")])
        self.assertEqual(p["last_active"], "2025-05-04")

    def test2_several_share_surname(self):
        found, total, _ = lookup(SMITHS, "smith", "John", "2010-05-31")
        self.assertEqual((len(found), total), (4, 5))            # Smith-Jones excluded, still counted in total

    def test3_first_name_distinguishes(self):
        _, _, (status, m) = lookup(SMITHS, "Smith", "Jane", "2010-05-31")
        self.assertEqual((status, [x[0]["id"] for x in m]), ("verified", ["1003"]))

    def test4_dob_identifies_intended_john(self):
        _, _, (status, m) = lookup(SMITHS, "Smith", "John", "2012-01-02")
        self.assertEqual((status, [x[0]["id"] for x in m]), ("verified", ["1002"]))

    def test5_dob_mismatch_is_not_verified(self):
        _, _, (status, m) = lookup(SMITHS, "Smith", "John", "1999-09-09")
        self.assertEqual(status, "partial")
        johns = [(p["id"], dob_ok) for p, first_ok, dob_ok in m if first_ok]
        self.assertEqual(johns, [("1001", False), ("1002", False)])
        self.assertIn("does NOT match", t.describe_verification(True, False))

    def test5b_dob_unavailable_is_not_claimed_verified(self):
        _, _, (status, m) = lookup(SMITHS, "Smith", "Sam", "2010-05-31")
        sam = [(p["id"], dob_ok) for p, first_ok, dob_ok in m if first_ok]
        self.assertEqual(sam, [("1004", None)])
        self.assertNotIn("Verified", t.describe_verification(True, None))

    def test6_no_matching_player(self):
        empty_table = page()                                       # headers, no rows
        no_table = "<html><body><h4>SEARCH PLAYER</h4>Search results for: \"zzz\"</body></html>"
        for html in (empty_table, no_table):
            found, total, _ = lookup(html, "Zzz", "A", "2000-01-01")
            self.assertEqual((found, total), ([], 0))
        found, total, _ = lookup(page(row(1, "9", "Smith-Jones, A (Anna)", "2000-01-01")), "Smith", "Anna", "2000-01-01")
        self.assertEqual((found, total), ([], 1))                  # partial match only -> no exact match
        _, _, (status, m) = lookup(SMITHS, "Smith", "Zed", "1990-01-01")
        self.assertEqual((status, m), ("none", []))                # surname alone never shows ratings

    def test7_missing_rating_vs_zero(self):
        html = page(row(1, "5", "Zero, Z (Zed)", "2001-01-01", main="", std="0", rapid="-", blitz="1200"))
        found, _, _ = lookup(html, "Zero", "Zed", "2001-01-01")
        ratings = dict(t.extract_player_ratings(found[0]))
        self.assertEqual(ratings, {"Main": None, "Standard": "0", "Rapid": None, "Blitz": "1200"})

    def test8_website_unavailable(self):
        for error, expected in [(requests.Timeout(), "too long"), (requests.ConnectionError(), "Could not connect")]:
            with mock.patch.object(t, "MIN_SECONDS_BETWEEN_REQUESTS", 0):
                with self.assertRaises(t.SiteError) as ctx:
                    t.search_players_by_surname(FakeSession(error=error), "Smith")
            self.assertIn(expected, str(ctx.exception))
        with mock.patch.object(t, "MIN_SECONDS_BETWEEN_REQUESTS", 0):
            with self.assertRaises(t.SiteError) as ctx:
                t.search_players_by_surname(FakeSession(FakeResponse("", status=500)), "Smith")
        self.assertIn("500", str(ctx.exception))

    def test9_unexpected_data(self):
        debug = Path(tempfile.mkdtemp()) / "last_response.html"
        bad_pages = ["<html><body>Maintenance</body></html>",
                     "<html><body>Search results for: x<table><tr><th>Chess SA id</th><th>Name</th>"
                     "<th>Dob</th></tr></table>x</body></html>"]       # has table, but no rating columns -> unusable
        with mock.patch.object(t, "DEBUG_FILE", debug), mock.patch.object(t, "MIN_SECONDS_BETWEEN_REQUESTS", 0):
            for html in bad_pages:
                with self.assertRaises(t.SiteError):
                    t.search_players_by_surname(FakeSession(FakeResponse(html)), "Smith")
            self.assertTrue(debug.exists())                          # raw page kept for investigation

    def test9b_short_rows_are_skipped(self):
        html = page(row(1, "1", "Ok, O (Ola)", "2000-01-01"), "<tr><td>spacer</td></tr>")
        found, total, _ = lookup(html, "Ok", "Ola", "2000-01-01")
        self.assertEqual((len(found), total), (1, 1))

    def test10_name_without_brackets_has_no_first_name(self):
        found, _, (status, m) = lookup(page(row(1, "7", "Brackets, B", "2000-01-01")), "Brackets", "Bob", "2000-01-01")
        self.assertEqual(found[0]["first_name"], "")
        self.assertEqual(status, "partial")                          # DOB matches, first name unknown
        self.assertIsNone(m[0][1])
        self.assertIn("first name not shown", t.describe_verification(None, True))

    def test11_request_uses_the_sites_form_field(self):
        session = FakeSession(FakeResponse(page()))
        with mock.patch.object(t, "MIN_SECONDS_BETWEEN_REQUESTS", 0):
            t.search_players_by_surname(session, "Pillay")
        url, kwargs = session.calls[0]
        self.assertEqual(url, "https://ratings.chessa.co.za/playersearch.php")
        self.assertEqual(kwargs["data"], {"speler": "Pillay"})
        self.assertTrue(kwargs["timeout"])

    def test12_accents_and_name_parsing(self):
        found, _, (status, _m) = lookup(page(row(1, "8", "Van der Merwe, H (Hendr\u00e9 Pieter)", "2001-03-04")),
                                        "van der merwe", "Hendre", "2001-03-04")
        self.assertEqual(status, "verified")
        self.assertEqual(t.parse_name("Karsten, JD (Jan Daniel)"), ("Karsten", "JD", "Jan Daniel"))

    def test13_first_name_rules_and_bad_date_input(self):
        self.assertFalse(t.first_names_match("Johnny", "John"))
        self.assertTrue(t.first_names_match("Jan Daniel", "Jan"))
        with self.assertRaises(ValueError):
            t.parse_input_date("08/04/2004")
        self.assertIsNone(t.parse_input_date(""))


if __name__ == "__main__":
    unittest.main(verbosity=2)
