#include "pgn_parser.hpp"

#include <fstream>
#include <sstream>
#include <algorithm>
#include <regex>
#include <cmath>
#include <iomanip>

namespace cgd {

bool PgnParser::parseFile(const std::string& filepath) {
    std::ifstream file(filepath);
    if (!file.is_open()) {
        warnings_.push_back("Cannot open file: " + filepath);
        return false;
    }

    std::stringstream ss;
    ss << file.rdbuf();
    return parseString(ss.str());
}

bool PgnParser::parseString(const std::string& content) {
    games_.clear();
    warnings_.clear();

    size_t pos = 0;
    while (pos < content.length()) {
        // Find the start of a game (tag section starts with '[')
        pos = content.find('[', pos);
        if (pos == std::string::npos) break;

        // Find the end of the movetext (next game or EOF)
        size_t nextGame = content.find("\n[", pos + 1);
        std::string gameText;
        if (nextGame != std::string::npos) {
            gameText = content.substr(pos, nextGame - pos);
            pos = nextGame + 1;
        } else {
            gameText = content.substr(pos);
            pos = content.length();
        }

        auto game = parseGame(gameText);
        if (game.has_value()) {
            games_.push_back(game.value());
        }
    }

    return !games_.empty();
}

std::unordered_map<std::string, std::string>
PgnParser::parseTags(const std::string& tagSection) {
    std::unordered_map<std::string, std::string> tags;
    std::regex tagRegex(R"rc(\[(\w+)\s+"([^"]*)"\])rc");
    std::sregex_iterator it(tagSection.begin(), tagSection.end(), tagRegex);
    std::sregex_iterator end;

    for (; it != end; ++it) {
        tags[(*it)[1].str()] = (*it)[2].str();
    }

    return tags;
}

std::vector<std::string>
PgnParser::extractMoves(const std::string& movetext) {
    std::vector<std::string> moves;

    // Remove variations (parenthetical content)
    std::regex variationRegex(R"rc\([^)]*\))rc");
    std::string cleaned = std::regex_replace(movetext, variationRegex, " ");

    // Remove NAGs
    std::regex nagRegex(R"rc\$\d+)rc");
    cleaned = std::regex_replace(cleaned, nagRegex, " ");

    // Remove time stamps {[%clk ...]}
    std::regex clockRegex(R"rc(\{[^}]*\}))rc");
    cleaned = std::regex_replace(cleaned, clockRegex, " ");

    // Tokenize
    std::stringstream ss(cleaned);
    std::string token;
    while (ss >> token) {
        // Skip move numbers (e.g., "1.", "1...", "2.")
        if (std::regex_match(token, std::regex(R"(^\d+(\.\.\.)?\.?$)"))) continue;

        // Skip result tokens
        if (token == "1-0" || token == "0-1" || token == "1/2-1/2" || token == "*") continue;

        moves.push_back(token);
    }

    return moves;
}

bool PgnParser::isMove(const std::string& token) const {
    if (token.empty()) return false;

    // Castling
    if (token == "O-O" || token == "O-O-O" ||
        token == "0-0" || token == "0-0-0") return true;

    // SAN piece move: e.g., "Nf3", "Qh5+", "Rae1", "exd5", "e4"
    std::regex sanRegex(
        "^[NBRQK]?[a-h]?[1-8]?x?[a-h][1-8](?:=?[NBRQ])?[+#]?$"
    );
    return std::regex_match(token, sanRegex);
}

std::string PgnParser::inferResult(const std::string& movetext) const {
    if (movetext.find("1-0") != std::string::npos) return "1-0";
    if (movetext.find("0-1") != std::string::npos) return "0-1";
    if (movetext.find("1/2-1/2") != std::string::npos) return "1/2-1/2";
    return "*";
}

std::optional<GameResult> PgnParser::parseGame(const std::string& gameText) {
    // Split into tag section and movetext
    size_t firstMove = gameText.find_first_of('\n');
    if (firstMove == std::string::npos) firstMove = gameText.length();

    // Find where tags end (after the closing ] of the last tag)
    size_t tagEnd = gameText.find_last_of(']');
    if (tagEnd == std::string::npos) {
        warnings_.push_back("Malformed game: no tag section found");
        return std::nullopt;
    }

    std::string tagSection = gameText.substr(0, tagEnd + 1);
    std::string movetext = gameText.substr(tagEnd + 1);

    auto tags = parseTags(tagSection);

    if (tags.find("White") == tags.end() || tags.find("Black") == tags.end()) {
        warnings_.push_back("Missing White or Black tag, skipping game");
        return std::nullopt;
    }

    auto moves = extractMoves(movetext);
    int moveCount = 0;
    for (const auto& move : moves) {
        if (isMove(move)) moveCount++;
    }

    GameResult result;
    result.event = tags["Event"];
    result.site = tags["Site"];
    result.date = tags["Date"];
    result.round = tags["Round"];
    result.white = tags["White"];
    result.black = tags["Black"];
    result.result = tags.count("Result") ? tags["Result"] : inferResult(movetext);
    result.moves = std::move(moves);
    result.move_count = moveCount;
    result.eco = tags.count("ECO") ? tags["ECO"] : "";

    result.white_elo = tags.count("WhiteElo") ? std::stoi(tags["WhiteElo"]) : 0;
    result.black_elo = tags.count("BlackElo") ? std::stoi(tags["BlackElo"]) : 0;

    return result;
}

std::unordered_map<std::string, PlayerStats>
PgnParser::computePlayerStats() const {
    std::unordered_map<std::string, PlayerStats> stats;

    for (const auto& game : games_) {
        // White player
        if (stats.find(game.white) == stats.end()) {
            stats[game.white] = PlayerStats{game.white};
        }
        auto& whiteStats = stats[game.white];
        whiteStats.games++;

        if (game.result == "1-0") {
            whiteStats.wins++;
        } else if (game.result == "0-1") {
            whiteStats.losses++;
        } else {
            whiteStats.draws++;
        }

        if (game.black_elo > 0) {
            whiteStats.total_opponent_rating += game.black_elo;
            whiteStats.opponent_count++;
        }

        // Black player
        if (stats.find(game.black) == stats.end()) {
            stats[game.black] = PlayerStats{game.black};
        }
        auto& blackStats = stats[game.black];
        blackStats.games++;

        if (game.result == "0-1") {
            blackStats.wins++;
        } else if (game.result == "1-0") {
            blackStats.losses++;
        } else {
            blackStats.draws++;
        }

        if (game.white_elo > 0) {
            blackStats.total_opponent_rating += game.white_elo;
            blackStats.opponent_count++;
        }
    }

    return stats;
}

std::string PgnParser::generateReport() const {
    std::ostringstream report;
    auto playerStats = computePlayerStats();

    report << "=== PGN Analysis Report ===\n";
    report << "Games parsed: " << games_.size() << "\n";
    report << "Unique players: " << playerStats.size() << "\n\n";

    // Sort players by score
    std::vector<std::pair<std::string, PlayerStats>> sorted(
        playerStats.begin(), playerStats.end()
    );

    std::sort(sorted.begin(), sorted.end(),
        [](const auto& a, const auto& b) {
            double scoreA = a.second.wins + a.second.draws * 0.5;
            double scoreB = b.second.wins + b.second.draws * 0.5;
            return scoreA > scoreB;
        }
    );

    report << std::left << std::setw(25) << "Player"
           << std::right << std::setw(6) << "Games"
           << std::setw(6) << "Wins"
           << std::setw(6) << "Loss"
           << std::setw(6) << "Draw"
           << std::setw(10) << "Score"
           << std::setw(12) << "AvgOpp" << "\n";
    report << std::string(65, '-') << "\n";

    for (const auto& [name, ps] : sorted) {
        double score = ps.wins + ps.draws * 0.5;
        double avgOpp = ps.opponent_count > 0
            ? ps.total_opponent_rating / ps.opponent_count : 0.0;

        report << std::left << std::setw(25) << name.substr(0, 24)
               << std::right << std::setw(6) << ps.games
               << std::setw(6) << ps.wins
               << std::setw(6) << ps.losses
               << std::setw(6) << ps.draws
               << std::setw(10) << std::fixed << std::setprecision(1) << score
               << std::setw(12) << std::fixed << std::setprecision(0) << avgOpp
               << "\n";
    }

    if (sorted.size() > 20) {
        report << "\n... (" << (sorted.size() - 20) << " more players not shown)\n";
    }

    // Warnings
    if (!warnings_.empty()) {
        report << "\nWarnings:\n";
        for (const auto& w : warnings_) {
            report << "  [!] " << w << "\n";
        }
    }

    return report.str();
}

} // namespace cgd
