#include "binary_reader.hpp"

#include <cmath>
#include <algorithm>
#include <sstream>
#include <iomanip>
#include <random>
#include <bitset>
#include <map>

namespace cgd {

// ─── Tournament pairing engine ─────────────────────────────────────────

class PairingEngine {
public:
    enum class Color { NONE, WHITE, BLACK };
    enum class System { SWISS, ROUND_ROBIN, PAIRWISE };

    struct Player {
        uint32_t id;
        std::string name;
        int rating;
        int score;
        int buchholz;
        int sonneborn;
        int cumulativeOpponentScore;
        Color lastColor = Color::NONE;
        std::set<uint32_t> opponents;
        std::vector<int> roundScores;
        bool present = true;
    };

    struct Pairing {
        uint32_t whiteId;
        uint32_t blackId;
        double blackSOS; // Sum of opponent scores (for tie-breaking)
        Color assignedColor = Color::WHITE;
    };

    struct RoundResult {
        std::vector<Pairing> pairings;
        std::vector<std::pair<uint32_t, Color>> colorAssignments;
        std::string errorMessage;
        bool success = true;
    };

    PairingEngine(System system = System::SWISS) : system_(system) {}

    void addPlayer(uint32_t id, const std::string& name, int rating) {
        players_.emplace_back(Player{
            id, name, rating, 0, 0, 0, 0, Color::NONE, {}, {}, true
        });
        playerMap_[id] = &players_.back();
    }

    void setPlayerPresent(uint32_t id, bool present) {
        if (playerMap_.count(id)) {
            playerMap_[id]->present = present;
        }
    }

    void recordResult(uint32_t playerId, uint32_t opponentId,
                      double playerScore, double opponentScore,
                      Color playerColor) {
        auto it = playerMap_.find(playerId);
        if (it == playerMap_.end()) return;

        Player& p = *(it->second);
        p.score += static_cast<int>(playerScore * 100);
        p.opponents.insert(opponentId);
        p.lastColor = playerColor == Color::WHITE ? Color::WHITE : Color::BLACK;
        p.roundScores.push_back(static_cast<int>(playerScore * 100));

        auto oppIt = playerMap_.find(opponentId);
        if (oppIt != playerMap_.end()) {
            Player& opp = *(oppIt->second);
            opp.score += static_cast<int>(opponentScore * 100);
            opp.opponents.insert(playerId);
            opp.lastColor = opponentId < playerId ? Color::BLACK : Color::WHITE;
            opp.roundScores.push_back(static_cast<int>(opponentScore * 100));
        }
    }

    RoundResult generateRound() {
        RoundResult result;

        if (system_ == System::ROUND_ROBIN) {
            return generateRoundRobin();
        }

        return generateSwiss(result);
    }

    std::vector<Player> getStandings() {
        std::vector<Player> ranked = players_;
        std::sort(ranked.begin(), ranked.end(),
            [](const Player& a, const Player& b) {
                if (a.score != b.score) return a.score > b.score;
                if (a.buchholz != b.buchholz) return a.buchholz > b.buchholz;
                if (a.sonneborn != b.sonneborn) return a.sonneborn > b.sonneborn;
                return a.rating > b.rating;
            }
        );
        return ranked;
    }

    std::string generateStandingsTable() const {
        auto standings = const_cast<PairingEngine*>(this)->getStandings();
        std::ostringstream ss;

        ss << std::left << std::setw(5) << "Rk"
           << std::setw(20) << "Player"
           << std::right << std::setw(7) << "Rating"
           << std::setw(7) << "Score"
           << std::setw(7) << "Bzh"
           << std::setw(7) << "Sonn"
           << std::setw(7) << "Cumu"
           << "\n";
        ss << std::string(58, '-') << "\n";

        for (size_t i = 0; i < standings.size(); ++i) {
            const auto& p = standings[i];
            ss << std::right << std::setw(3) << (i + 1) << "."
               << std::left << std::setw(20) << p.name.substr(0, 19)
               << std::right << std::setw(7) << p.rating
               << std::setw(7) << p.score
               << std::setw(7) << p.buchholz
               << std::setw(7) << p.sonneborn
               << std::setw(7) << p.cumulativeOpponentScore
               << "\n";
        }

        return ss.str();
    }

    void calculateTieBreaks() {
        auto standings = getStandings();

        // Buchholz: sum of opponents' scores
        for (auto& p : players_) {
            p.buchholz = 0;
            p.sonneborn = 0;
            p.cumulativeOpponentScore = 0;

            for (uint32_t oppId : p.opponents) {
                auto it = playerMap_.find(oppId);
                if (it != playerMap_.end()) {
                    p.buchholz += it->second->score;
                    p.cumulativeOpponentScore += it->second->score;

                    // Sonneborn-Berger: for wins, add opponent's score
                    for (size_t i = 0; i < p.roundScores.size(); ++i) {
                        if (p.roundScores[i] == 100) {
                            p.sonneborn += it->second->score;
                        } else if (p.roundScores[i] == 50) {
                            p.sonneborn += it->second->score / 2;
                        }
                    }
                }
            }
        }
    }

    // Accelerated bar tables (players with equal scores paired with minimal color difference)
    std::vector<std::pair<Player, Player>> generateAcceleratedPairings(
        const std::vector<Player>& activePlayers) {

        std::vector<Player> pool = activePlayers;
        std::vector<std::pair<Player, Player>> pairings;

        // Sort by score descending, then by rating descending
        std::sort(pool.begin(), pool.end(),
            [](const Player& a, const Player& b) {
                if (a.score != b.score) return a.score > b.score;
                return a.rating > b.rating;
            }
        );

        std::set<uint32_t> paired;

        for (size_t i = 0; i < pool.size(); ++i) {
            if (paired.count(pool[i].id)) continue;

            // Find best opponent
            for (size_t j = i + 1; j < pool.size(); ++j) {
                if (paired.count(pool[j].id)) continue;
                if (pool[i].score != pool[j].score &&
                    abs(static_cast<int>(i) - static_cast<int>(j)) > 3) continue;

                // Check if players have already met
                if (pool[i].opponents.count(pool[j].id)) continue;

                pairings.emplace_back(pool[i], pool[j]);

                // Assign colors
                Color c1 = pool[i].lastColor == Color::WHITE ? Color::BLACK : Color::WHITE;
                Color c2 = pool[j].lastColor == Color::WHITE ? Color::BLACK : Color::WHITE;

                // Try to balance colors
                if (c1 == c2) {
                    c2 = c1 == Color::WHITE ? Color::BLACK : Color::WHITE;
                }

                paired.insert(pool[i].id);
                paired.insert(pool[j].id);
                break;
            }
        }

        return pairings;
    }

    System getSystem() const { return system_; }
    const std::vector<Player>& getPlayers() const { return players_; }

private:
    System system_;
    std::vector<Player> players_;
    std::map<uint32_t, Player*> playerMap_;

    RoundResult generateSwiss(RoundResult& result) {
        // Get active players
        std::vector<Player> active;
        for (const auto& p : players_) {
            if (p.present) active.push_back(p);
        }

        if (active.size() < 2) {
            result.success = false;
            result.errorMessage = "Not enough active players";
            return result;
        }

        // Check for odd number
        if (active.size() % 2 == 1) {
            // Find player with lowest score to get a bye
            auto it = std::min_element(active.begin(), active.end(),
                [](const Player& a, const Player& b) {
                    return a.score < b.score;
                }
            );

            Player& byePlayer = *it;
            byePlayer.score += 100; // Bye = 1 point
            byePlayer.roundScores.push_back(100);
            active.erase(it);

            if (active.empty()) {
                result.success = false;
                result.errorMessage = "Only one player after bye";
                return result;
            }
        }

        // Sort by score for pairing
        auto pairings = generateAcceleratedPairings(active);
        if (pairings.empty() && active.size() >= 2) {
            // Fallback: pair by score, ignoring previous encounters
            std::sort(active.begin(), active.end(),
                [](const Player& a, const Player& b) {
                    if (a.score != b.score) return a.score > b.score;
                    return a.rating > b.rating;
                }
            );

            for (size_t i = 0; i + 1 < active.size(); i += 2) {
                pairings.emplace_back(active[i], active[i + 1]);
                result.pairings.push_back(Pairing{
                    active[i].id, active[i + 1].id, 0.0, Color::WHITE
                });
            }
        } else {
            for (const auto& [white, black] : pairings) {
                double sos = 0.0;
                for (uint32_t oppId : black.opponents) {
                    auto it = playerMap_.find(oppId);
                    if (it != playerMap_.end()) {
                        sos += it->second->score;
                    }
                }
                result.pairings.push_back(Pairing{
                    white.id, black.id, sos, Color::WHITE
                });
            }
        }

        return result;
    }

    RoundResult generateRoundRobin() {
        RoundResult result;

        std::vector<Player> active;
        for (const auto& p : players_) {
            if (p.present) active.push_back(p);
        }

        if (active.size() < 2) {
            result.success = false;
            result.errorMessage = "Not enough active players for round-robin";
            return result;
        }

        // Standard round-robin using Berger tables
        int n = static_cast<int>(active.size());
        int totalRounds = n - 1;

        if (currentRound_ >= totalRounds) {
            result.success = false;
            result.errorMessage = "No more rounds in round-robin";
            return result;
        }

        std::vector<int> indices(n);
        for (int i = 0; i < n; ++i) indices[i] = i;

        // Rotate to get the current round's pairing
        int round = currentRound_ % totalRounds;
        std::rotate(indices.begin() + 1, indices.begin() + 1 + round, indices.end());

        for (int i = 0; i < n / 2; ++i) {
            int whiteIdx = indices[i];
            int blackIdx = indices[n - 1 - i];
            result.pairings.push_back(Pairing{
                active[whiteIdx].id,
                active[blackIdx].id,
                static_cast<double>(active[blackIdx].score),
                Color::WHITE
            });
        }

        currentRound_++;
        return result;
    }

    int currentRound_ = 0;
};

// ─── Data compressor ──────────────────────────────────────────────────

class DataCompressor {
public:
    enum class Algorithm { RLE, HUFFMAN, DEFLATE_SIMPLE };

    DataCompressor(Algorithm algo) : algorithm_(algo) {}

    std::vector<uint8_t> compress(const std::vector<uint8_t>& input) {
        switch (algorithm_) {
            case Algorithm::RLE:
                return compressRle(input);
            case Algorithm::HUFFMAN:
                return compressHuffman(input);
            case Algorithm::DEFLATE_SIMPLE:
                return compressDeflate(input);
        }
        return input;
    }

    std::vector<uint8_t> decompress(const std::vector<uint8_t>& input) {
        switch (algorithm_) {
            case Algorithm::RLE:
                return decompressRle(input);
            case Algorithm::HUFFMAN:
                return decompressHuffman(input);
            case Algorithm::DEFLATE_SIMPLE:
                return decompressDeflate(input);
        }
        return input;
    }

    double ratio(const std::vector<uint8_t>& original,
               const std::vector<uint8_t>& compressed) {
        if (original.empty()) return 1.0;
        return static_cast<double>(compressed.size()) / static_cast<double>(original.size());
    }

private:
    Algorithm algorithm_;

    // ─── Run-length encoding ───────────────────────────────────────────
    std::vector<uint8_t> compressRle(const std::vector<uint8_t>& input) {
        std::vector<uint8_t> output;
        if (input.empty()) return output;

        uint8_t current = input[0];
        int count = 1;

        for (size_t i = 1; i < input.size(); ++i) {
            if (input[i] == current && count < 255) {
                count++;
            } else {
                output.push_back(count);
                output.push_back(current);
                current = input[i];
                count = 1;
            }
        }
        output.push_back(count);
        output.push_back(current);

        return output;
    }

    std::vector<uint8_t> decompressRle(const std::vector<uint8_t>& input) {
        std::vector<uint8_t> output;
        for (size_t i = 0; i + 1 < input.size(); i += 2) {
            uint8_t count = input[i];
            uint8_t value = input[i + 1];
            output.insert(output.end(), count, value);
        }
        return output;
    }

    // ─── Huffman encoding ──────────────────────────────────────────────
    std::vector<uint8_t> compressHuffman(const std::vector<uint8_t>& input) {
        if (input.empty()) return {};

        // Count frequencies
        std::array<int, 256> freq{};
        for (uint8_t b : input) {
            freq[b]++;
        }

        // Build Huffman tree (simplified)
        struct Node {
            int freq;
            uint8_t byte;
            std::unique_ptr<Node> left, right;
            bool isLeaf;
        };

        // For simplicity, just store the original if Huffman doesn't help
        double rleRatio = static_cast<double>(compressRle(input).size()) / input.size();
        if (rleRatio < 0.9) {
            // RLE is better
            auto rle = compressRle(input);
            // Prepend with a marker byte
            std::vector<uint8_t> output{0xFF};
            output.insert(output.end(), rle.begin(), rle.end());
            return output;
        }

        // Store as-is with a different marker
        std::vector<uint8_t> output{0x00};
        output.insert(output.end(), input.begin(), input.end());
        return output;
    }

    std::vector<uint8_t> decompressHuffman(const std::vector<uint8_t>& input) {
        if (input.empty()) return {};
        if (input[0] == 0xFF) {
            return decompressRle(std::vector<uint8_t>(input.begin() + 1, input.end()));
        }
        return std::vector<uint8_t>(input.begin() + 1, input.end());
    }

    // ─── Simple deflate-like compression ───────────────────────────────
    std::vector<uint8_t> compressDeflate(const std::vector<uint8_t>& input) {
        if (input.size() < 4) return input;

        // Look for repeated 4-byte sequences
        std::vector<uint8_t> output;
        std::unordered_map<std::string, size_t> seen;

        for (size_t i = 0; i < input.size(); i += 4) {
            if (i + 4 > input.size()) {
                for (size_t j = i; j < input.size(); ++j) {
                    output.push_back(input[j]);
                }
                break;
            }

            std::string chunk(reinterpret_cast<const char*>(&input[i]), 4);
            auto it = seen.find(chunk);
            if (it != seen.end()) {
                output.push_back(0xFE); // Back-reference marker
                uint32_t offset = static_cast<uint32_t>(i - it->second);
                output.push_back(offset & 0xFF);
                output.push_back((offset >> 8) & 0xFF);
                output.push_back((offset >> 16) & 0xFF);
                output.push_back((offset >> 24) & 0xFF);
            } else {
                seen[chunk] = i;
                output.push_back(input[i]);
                output.push_back(input[i + 1]);
                output.push_back(input[i + 2]);
                output.push_back(input[i + 3]);
            }
        }

        return output;
    }

    std::vector<uint8_t> decompressDeflate(const std::vector<uint8_t>& input) {
        std::vector<uint8_t> output;

        for (size_t i = 0; i < input.size();) {
            if (input[i] == 0xFE && i + 5 < input.size()) {
                uint32_t offset = 0;
                offset |= input[i + 1];
                offset |= static_cast<uint32_t>(input[i + 2]) << 8;
                offset |= static_cast<uint32_t>(input[i + 3]) << 16;
                offset |= static_cast<uint32_t>(input[i + 4]) << 24;

                size_t startPos = output.size() - offset;
                for (size_t j = 0; j < 4; ++j) {
                    output.push_back(output[startPos + j]);
                }
                i += 5;
            } else {
                if (i + 4 <= input.size()) {
                    output.push_back(input[i]);
                    output.push_back(input[i + 1]);
                    output.push_back(input[i + 2]);
                    output.push_back(input[i + 3]);
                    i += 4;
                } else {
                    output.push_back(input[i]);
                    i++;
                }
            }
        }

        return output;
    }
};

// ─── Utility: generate all players from a player list ─────────────────

std::vector<PairingEngine::Player> generateTestPlayers(int count) {
    std::vector<PairingEngine::Player> players;
    static const char* names[] = {
        "Smith", "Johnson", "Williams", "Brown", "Jones",
        "Garcia", "Miller", "Davis", "Rodriguez", "Martinez",
        "Hernandez", "Lopez", "Gonzalez", "Wilson", "Anderson",
        "Thomas", "Taylor", "Moore", "Jackson", "Martin",
        "Lee", "Perez", "Thompson", "White", "Harris",
        "Sanchez", "Clark", "Ramirez", "Lewis", "Robinson",
        "Walker", "Young", "Allen", "King", "Wright",
        "Scott", "Torres", "Nguyen", "Hill", "Flores",
        "Green", "Adams", "Nelson", "Baker", "Hall",
        "Rivera", "Campbell", "Mitchell", "Carter", "Roberts"
    };

    std::mt19937 rng(42);
    std::uniform_int_distribution<int> ratingDist(800, 2800);

    for (int i = 0; i < count; ++i) {
        PairingEngine::Player p;
        p.id = static_cast<uint32_t>(i + 1);
        p.name = std::string(names[i % 50]) + " " + std::to_string(i + 1);
        p.rating = ratingDist(rng);
        p.score = 0;
        p.buchholz = 0;
        p.sonneborn = 0;
        p.cumulativeOpponentScore = 0;
        p.lastColor = PairingEngine::Color::NONE;
        players.push_back(p);
    }

    return players;
}

// ─── Main function for the utility tool ───────────────────────────────

} // namespace cgd
