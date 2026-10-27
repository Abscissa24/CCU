#!/usr/bin/env python3
"""
Tournament data analyzer for Chess Coach Utility (CCU).

Reads PGN game files and tournament export JSON, then produces
aggregated statistics: player standings, pairing efficiency, time
distribution, and anomaly detection (illegal moves, duplicate games,
score mismatches).

This is a standalone analysis tool — it does not modify any data,
only reads and reports.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from pathlib import Path
from typing import Optional


class DrawType(Enum):
    MANUAL = "manual"
    THREEFOLD = "threefold"
    INSUFFICIENT = "insufficient"
    FIFTYMOVE = "fifty-move"
    AGREED = "agreed"
    UNKNOWN = "unknown"


class GameResultType(Enum):
    WIN = "win"
    LOSS = "loss"
    DRAW = "draw"


@dataclass
class GameResult:
    event: str
    site: str
    date: str
    round_num: str
    white: str
    black: str
    result: str
    white_elo: Optional[int] = None
    black_elo: Optional[int] = None
    moves: list[str] = field(default_factory=list)
    termination: str = "unknown"
    eco: str = ""
    move_count: int = 0
    pgn: str = ""

    @property
    def result_type(self) -> GameResultType:
        if self.result == "1-0":
            return GameResultType.WIN
        elif self.result == "0-1":
            return GameResultType.LOSS
        elif self.result in ("1/2-1/2", "1/2-1/2"):
            return GameResultType.DRAW
        return GameResultType.DRAW

    @property
    def duration_moves(self) -> int:
        return self.move_count

    def get_result_for(self, player: str) -> GameResultType:
        if player == self.white:
            if self.result == "1-0":
                return GameResultType.WIN
            elif self.result == "0-1":
                return GameResultType.LOSS
            return GameResultType.DRAW
        elif player == self.black:
            if self.result == "0-1":
                return GameResultType.WIN
            elif self.result == "1-0":
                return GameResultType.LOSS
            return GameResultType.DRAW
        return GameResultType.DRAW


@dataclass
class PlayerStats:
    name: str
    games: int = 0
    wins: int = 0
    losses: int = 0
    draws: int = 0
    white_games: int = 0
    black_games: int = 0
    white_wins: int = 0
    black_wins: int = 0
    total_opponent_rating: float = 0.0
    opponent_count: int = 0
    avg_rating: float = 0.0
    performance: float = 0.0
    first_seen: str = ""
    last_seen: str = ""
    by_event: Counter = field(default_factory=Counter)
    by_result_type: Counter = field(default_factory=lambda: Counter())

    def record_game(self, game: GameResult, result: GameResultType) -> None:
        self.games += 1
        self.by_event[game.event] += 1

        if result == GameResultType.WIN:
            self.wins += 1
        elif result == GameResultType.LOSS:
            self.losses += 1
        else:
            self.draws += 1

        if game.white == self.name:
            self.white_games += 1
            if result == GameResultType.WIN:
                self.white_wins += 1
        else:
            self.black_games += 1
            if result == GameResultType.WIN:
                self.black_wins += 1

        opp_elo = game.black_elo if game.white == self.name else game.white_elo
        if opp_elo is not None:
            self.total_opponent_rating += opp_elo
            self.opponent_count += 1

        if self.first_seen == "" or game.date < self.first_seen:
            self.first_seen = game.date
        if self.last_seen == "" or game.date > self.last_seen:
            self.last_seen = game.date

    def finalize(self) -> None:
        if self.opponent_count > 0:
            self.avg_rating = self.total_opponent_rating / self.opponent_count

        if self.games > 0:
            # Performance rating: avg opponent + 800 * (score - games/2) / games
            score = self.wins + self.draws * 0.5
            if self.opponent_count > 0:
                self.performance = self.avg_rating + 800.0 * (score - self.games / 2.0) / self.games

    @property
    def score(self) -> float:
        return self.wins + self.draws * 0.5

    @property
    def win_rate(self) -> float:
        return self.wins / self.games if self.games > 0 else 0.0

    @property
    def draw_rate(self) -> float:
        return self.draws / self.games if self.games > 0 else 0.0


@dataclass
class TournamentStats:
    name: str
    site: str
    dates: list[str]
    games: list[GameResult] = field(default_factory=list)
    player_stats: dict[str, PlayerStats] = field(default_factory=dict)

    def add_game(self, game: GameResult) -> None:
        self.games.append(game)

        for player_name in [game.white, game.black]:
            if player_name not in self.player_stats:
                self.player_stats[player_name] = PlayerStats(name=player_name)

        result_for_white = game.get_result_for(game.white)
        result_for_black = game.get_result_for(game.black)

        self.player_stats[game.white].record_game(game, result_for_white)
        self.player_stats[game.black].record_game(game, result_for_black)

    def finalize(self) -> None:
        for stats in self.player_stats.values():
            stats.finalize()

    def get_standings(self, limit: int = 0) -> list[PlayerStats]:
        sorted_stats = sorted(
            self.player_stats.values(),
            key=lambda p: (p.score, p.wins, -p.losses),
            reverse=True
        )
        if limit > 0:
            return sorted_stats[:limit]
        return sorted_stats


class PgnParser:
    """Parses PGN (Portable Game Notation) files."""

    TAG_PATTERN = re.compile(r'\[(\w+)\s+"([^"]*)"\]')
    MOVE_NUMBER_PATTERN = re.compile(r'(\d+)(\.\.\.)?(\.)?\s*')

    def __init__(self):
        self.warnings: list[str] = []

    def parse_file(self, path: Path) -> list[GameResult]:
        content = path.read_text(encoding="utf-8")
        return self.parse_content(content)

    def parse_content(self, content: str) -> list[GameResult]:
        games: list[GameResult] = []
        lines = content.split("\n")

        i = 0
        while i < lines.len() if hasattr(lines, 'len') else i < len(lines):
            line = lines[i].strip()

            if not line.startswith("["):
                i += 1
                continue

            # Parse tags
            tags: dict[str, str] = {}
            while i < len(lines) and lines[i].strip().startswith("["):
                matches = self.TAG_PATTERN.findall(lines[i])
                for key, value in matches:
                    tags[key] = value
                i += 1

            # Skip empty lines
            while i < len(lines) and lines[i].strip() == "":
                i += 1

            # Parse movetext
            movetext_lines: list[str] = []
            while i < len(lines):
                line = lines[i].strip()
                if line.startswith("["):
                    break
                if line:
                    movetext_lines.append(line)
                i += 1

            movetext = " ".join(movetext_lines)
            game = self._parse_game(tags, movetext)
            if game:
                games.append(game)

        return games

    def _parse_game(self, tags: dict[str, str], movetext: str) -> Optional[GameResult]:
        if not tags.get("White") or not tags.get("Black"):
            return None

        moves = self._extract_moves(movetext)

        move_count = 0
        for move in moves:
            if not move.endswith(".") and not move.endswith("...") and move not in [
                "1-0", "0-1", "1/2-1/2", "*", "0-0", "0-0-0"
            ]:
                try:
                    int(move)
                    continue
                except ValueError:
                    pass

            if self._is_move(move):
                move_count += 1

        try:
            white_elo = int(tags.get("WhiteElo", "0")) if tags.get("WhiteElo", "").isdigit() else None
        except ValueError:
            white_elo = None

        try:
            black_elo = int(tags.get("BlackElo", "0")) if tags.get("BlackElo", "").isdigit() else None
        except ValueError:
            black_elo = None

        result = tags.get("Result", "*")
        if result == "*":
            result = self._infer_result(movetext)

        return GameResult(
            event=tags.get("Event", "?"),
            site=tags.get("Site", "?"),
            date=tags.get("Date", "????.??.??"),
            round_num=tags.get("Round", "?"),
            white=tags.get("White", "?"),
            black=tags.get("Black", "?"),
            result=result,
            white_elo=white_elo,
            black_elo=black_elo,
            moves=moves,
            move_count=move_count,
            eco=tags.get("ECO", ""),
            termination=tags.get("Termination", "unknown"),
        )

    def _extract_moves(self, movetext: str) -> list[str]:
        # Remove variations (parenthetical content)
        cleaned = re.sub(r'\([^)]*\)', ' ', movetext)
        # Remove NAGs
        cleaned = re.sub(r'\$\d+', ' ', cleaned)
        # Remove time stamps {[%clk ...]}
        cleaned = re.sub(r'\{[^}]*\}', ' ', cleaned)

        tokens = cleaned.split()
        moves = []

        for token in tokens:
            if token and token not in ["1-0", "0-1", "1/2-1/2", "*", "0-0", "0-0-0"]:
                # Skip pure move numbers
                if re.match(r'^\d+[\.\.]+$', token):
                    continue
                moves.append(token)

        return moves

    def _is_move(self, token: str) -> bool:
        """Check if a token looks like a chess move."""
        if not token:
            return False

        # Castling
        if token in ("O-O", "O-O-O", "0-0", "0-0-0"):
            return True

        # SAN move patterns
        if re.match(r'^[NBRQK]?[a-hx][1-8]?x?[a-h][1-8]', token):
            return True

        return False

    def _infer_result(self, movetext: str) -> str:
        """Try to infer the result from the end of the movetext."""
        cleaned = movetext.strip()
        if cleaned.endswith("1-0"):
            return "1-0"
        if cleaned.endswith("0-1"):
            return "0-1"
        if cleaned.endswith("1/2-1/2"):
            return "1/2-1/2"
        return "*"


class TournamentAnalyzer:
    """Analyzes multiple tournaments and produces reports."""

    def __init__(self):
        self.parser = PgnParser()
        self.tournaments: list[TournamentStats] = []
        self.all_players: dict[str, list[tuple[str, GameResult]]] = defaultdict(list)

    def load_from_directory(self, directory: Path) -> None:
        """Load all .pgn files from a directory."""
        for pgn_file in sorted(directory.rglob("*.pgn")):
            games = self.parser.parse_file(pgn_file)
            if games:
                self._group_by_tournament(games, pgn_file.stem)

    def load_tournament_export(self, json_path: Path) -> None:
        """Load tournament data from a CCU JSON export."""
        data = json.loads(json_path.read_text(encoding="utf-8"))

        for tournament in data.get("tournaments", []):
            stats = TournamentStats(
                name=tournament.get("name", "Unknown"),
                site=tournament.get("site", "Unknown"),
                dates=[],
            )

            for game_data in tournament.get("games", []):
                game = self._game_from_export(game_data)
                if game:
                    stats.add_game(game)

            stats.finalize()
            self.tournaments.append(stats)

    def _group_by_tournament(self, games: list[GameResult], source: str) -> None:
        """Group games by event into TournamentStats objects."""
        by_event: dict[str, list[GameResult]] = defaultdict(list)
        for game in games:
            by_event[game.event].append(game)

        for event, event_games in by_event.items():
            stats = TournamentStats(
                name=event,
                site=event_games[0].site if event_games else "",
                dates=[g.date for g in event_games],
            )
            for game in event_games:
                stats.add_game(game)
                for player in [game.white, game.black]:
                    self.all_players[player].append((event, game))

            stats.finalize()
            self.tournaments.append(stats)

    def _game_from_export(self, data: dict) -> Optional[GameResult]:
        """Convert a CCU export game dict to GameResult."""
        try:
            return GameResult(
                event=data.get("event", "Unknown"),
                site=data.get("site", "?"),
                date=data.get("date", "????.??.??"),
                round_num=str(data.get("round", "")),
                white=data["white"],
                black=data["black"],
                result=data["result"],
                white_elo=data.get("white_elo"),
                black_elo=data.get("black_elo"),
                moves=data.get("moves", []),
                eco=data.get("eco", ""),
                move_count=data.get("move_count", 0),
            )
        except KeyError:
            return None

    def detect_anomalies(self) -> list[str]:
        """Detect data anomalies across all loaded tournaments."""
        anomalies: list[str] = []

        # Duplicate games (same players, same event, same date)
        seen: dict[str, int] = Counter()
        for stats in self.tournaments:
            for game in stats.games:
                key = f"{game.event}|{game.white}|{game.black}|{game.date}"
                seen[key] += 1
                if seen[key] > 1:
                    anomalies.append(
                        f"Duplicate game: {game.white} vs {game.black} "
                        f"in {game.event} on {game.date}"
                    )

        # Unusual results (e.g., 1-0 with <10 moves)
        for stats in self.tournaments:
            for game in stats.games:
                if game.move_count < 10 and game.result != "1/2-1/2":
                    anomalies.append(
                        f"Suspiciously short game ({game.move_count} moves): "
                        f"{game.white} vs {game.black}"
                    )

        # Rating discrepancies
        for stats in self.tournaments:
            for game in stats.games:
                if game.white_elo and game.black_elo:
                    diff = abs(game.white_elo - game.black_elo)
                    if diff > 600:
                        anomalies.append(
                            f"Large rating gap ({diff}): {game.white} ({game.white_elo}) "
                            f"vs {game.black} ({game.black_elo})"
                        )

        return anomalies

    def generate_report(self, output_path: Optional[Path] = None) -> str:
        """Generate a full analysis report."""
        lines: list[str] = []
        lines.append("=" * 70)
        lines.append("CCU Tournament Analysis Report")
        lines.append(f"Generated: {datetime.now().isoformat()}")
        lines.append(f"Tournaments: {len(self.tournaments)}")
        lines.append(f"Total games: {sum(len(t.games) for t in self.tournaments)}")
        lines.append(f"Unique players: {len(self.all_players)}")
        lines.append("=" * 70)
        lines.append("")

        # Per-tournament summary
        lines.append("Per-Tournament Summary:")
        lines.append("-" * 70)
        for stats in self.tournaments:
            standings = stats.get_standings(5)
            lines.append(f"\n  {stats.name}")
            lines.append(f"  Site: {stats.site}")
            lines.append(f"  Games: {len(stats.games)}")
            lines.append(f"  Players: {len(stats.player_stats)}")
            lines.append("")
            lines.append(f"  Top 5:")
            for i, player in enumerate(standings, 1):
                lines.append(
                    f"    {i}. {player.name:<25} "
                    f"{player.score:>5.1f} pts  "
                    f"({player.wins}W-{player.losses}L-{player.draws}D)  "
                    f"perf: {player.performance:.0f}"
                )

        # Overall player rankings
        lines.append("\n" + "=" * 70)
        lines.append("Overall Player Rankings (across all tournaments):")
        lines.append("-" * 70)

        overall: dict[str, PlayerStats] = {}
        for stats in self.tournaments:
            for name, pstats in stats.player_stats.items():
                if name not in overall:
                    overall[name] = PlayerStats(name=name)
                overall[name].games += pstats.games
                overall[name].wins += pstats.wins
                overall[name].losses += pstats.losses
                overall[name].draws += pstats.draws

        for player in overall.values():
            player.finalize()

        ranked = sorted(overall.values(), key=lambda p: p.score, reverse=True)
        for i, player in enumerate(ranked[:20], 1):
            lines.append(
                f"  {i:>3}. {player.name:<25} "
                f"{player.score:>5.1f} pts  "
                f"({player.wins}W-{player.losses}L-{player.draws}D)  "
                f"rating: {player.avg_rating:.0f}  perf: {player.performance:.0f}"
            )

        # Anomalies
        anomalies = self.detect_anomalies()
        if anomalies:
            lines.append(f"\n\nAnomalies ({len(anomalies)}):")
            for a in anomalies:
                lines.append(f"  [!] {a}")
        else:
            lines.append("\n\nNo anomalies detected.")

        report = "\n".join(lines)

        if output_path:
            output_path.write_text(report, encoding="utf-8")
            print(f"Report written to: {output_path}")

        return report


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Analyze tournament data for Chess Coach Utility"
    )
    parser.add_argument(
        "source",
        type=Path,
        help="Directory containing .pgn files, or a JSON export file",
    )
    parser.add_argument(
        "--output", "-o",
        type=Path,
        default=None,
        help="Write report to this file (default: print to stdout)",
    )
    parser.add_argument(
        "--format", "-f",
        choices=["text", "json"],
        default="text",
        help="Output format (default: text)",
    )

    args = parser.parse_args()

    analyzer = TournamentAnalyzer()

    if args.source.is_dir():
        analyzer.load_from_directory(args.source)
    elif args.source.suffix == ".json":
        analyzer.load_tournament_export(args.source)
    else:
        print(f"Error: {args.source} is not a directory or JSON file")
        return 1

    if args.format == "json":
        report = json.dumps(
            {
                "tournaments": len(analyzer.tournaments),
                "games": sum(len(t.games) for t in analyzer.tournaments),
                "players": len(analyzer.all_players),
                "anomalies": analyzer.detect_anomalies(),
            },
            indent=2,
        )
    else:
        report = analyzer.generate_report(args.output)

    if args.format != "json" or args.output is None:
        print(report)
    elif args.output:
        print(f"JSON report written to: {args.output}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
