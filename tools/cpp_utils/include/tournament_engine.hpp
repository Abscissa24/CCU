#pragma once

#include "binary_reader.hpp"
#include <set>
#include <unordered_map>
#include <array>
#include <memory>
#include <string>
#include <vector>
#include <cstdint>
#include <algorithm>
#include <random>

namespace cgd {

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
        double blackSOS;
        Color assignedColor = Color::WHITE;
    };

    struct RoundResult {
        std::vector<Pairing> pairings;
        std::vector<std::pair<uint32_t, Color>> colorAssignments;
        std::string errorMessage;
        bool success = true;
    };

    PairingEngine(System system = System::SWISS);

    void addPlayer(uint32_t id, const std::string& name, int rating);
    void setPlayerPresent(uint32_t id, bool present);
    void recordResult(uint32_t playerId, uint32_t opponentId,
                      double playerScore, double opponentScore,
                      Color playerColor);

    RoundResult generateRound();
    std::vector<Player> getStandings();
    std::string generateStandingsTable() const;
    void calculateTieBreaks();

    std::vector<std::pair<Player, Player>> generateAcceleratedPairings(
        const std::vector<Player>& activePlayers);

    System getSystem() const;
    const std::vector<Player>& getPlayers() const;

private:
    System system_;
    std::vector<Player> players_;
    std::map<uint32_t, Player*> playerMap_;
    int currentRound_ = 0;

    RoundResult generateSwiss(RoundResult& result);
    RoundResult generateRoundRobin();
};

class DataCompressor {
public:
    enum class Algorithm { RLE, HUFFMAN, DEFLATE_SIMPLE };

    DataCompressor(Algorithm algo);

    std::vector<uint8_t> compress(const std::vector<uint8_t>& input);
    std::vector<uint8_t> decompress(const std::vector<uint8_t>& input);
    double ratio(const std::vector<uint8_t>& original,
                 const std::vector<uint8_t>& compressed);

private:
    Algorithm algorithm_;

    std::vector<uint8_t> compressRle(const std::vector<uint8_t>& input);
    std::vector<uint8_t> decompressRle(const std::vector<uint8_t>& input);
    std::vector<uint8_t> compressHuffman(const std::vector<uint8_t>& input);
    std::vector<uint8_t> decompressHuffman(const std::vector<uint8_t>& input);
    std::vector<uint8_t> compressDeflate(const std::vector<uint8_t>& input);
    std::vector<uint8_t> decompressDeflate(const std::vector<uint8_t>& input);
};

std::vector<PairingEngine::Player> generateTestPlayers(int count);

} // namespace cgd
