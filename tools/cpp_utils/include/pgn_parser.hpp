#pragma once

#include <string>
#include <vector>
#include <unordered_map>
#include <optional>
#include <sstream>

namespace cgd {

struct GameResult {
    std::string event;
    std::string site;
    std::string date;
    std::string round;
    std::string white;
    std::string black;
    std::string result;
    int white_elo;
    int black_elo;
    std::vector<std::string> moves;
    int move_count;
    std::string eco;
};

struct PlayerStats {
    std::string name;
    int games = 0;
    int wins = 0;
    int losses = 0;
    int draws = 0;
    double total_opponent_rating = 0.0;
    int opponent_count = 0;
};

class PgnParser {
public:
    PgnParser() = default;
    ~PgnParser() = default;

    bool parseFile(const std::string& filepath);
    bool parseString(const std::string& content);

    const std::vector<GameResult>& getGames() const { return games_; }
    size_t getGameCount() const { return games_.size(); }

    std::unordered_map<std::string, PlayerStats> computePlayerStats() const;
    std::string generateReport() const;

private:
    std::vector<GameResult> games_;
    std::vector<std::string> warnings_;

    std::optional<GameResult> parseGame(const std::string& gameText);
    std::unordered_map<std::string, std::string> parseTags(const std::string& tagSection);
    std::vector<std::string> extractMoves(const std::string& movetext);
    bool isMove(const std::string& token) const;
    std::string inferResult(const std::string& movetext) const;
};

} // namespace cgd
