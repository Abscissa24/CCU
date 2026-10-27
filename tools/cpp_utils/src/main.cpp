#include "binary_reader.hpp"
#include "pgn_parser.hpp"

#include <iostream>
#include <fstream>
#include <vector>
#include <string>
#include <cstring>
#include <chrono>

namespace cgd {

class TournamentDataProcessor {
public:
    TournamentDataProcessor() = default;

    bool loadPlayers(const std::string& filepath) {
        BinaryReader reader(filepath);
        if (!reader.open()) {
            std::cerr << "Error: " << reader.getError() << "\n";
            return false;
        }

        players_ = reader.readAllRecords();
        reader.close();
        std::cout << "Loaded " << players_.size() << " player records\n";
        return true;
    }

    bool loadGames(const std::string& filepath) {
        parser_.parseFile(filepath);
        std::cout << "Parsed " << parser_.getGameCount() << " games\n";
        return parser_.getGameCount() > 0;
    }

    void printPlayerReport() const {
        std::cout << "\n=== Player Report ===\n";
        std::cout << "ID    Rating Games Name\n";
        std::cout << "----- ------ ----- ----\n";
        for (const auto& p : players_) {
            std::cout << p.id
                      << " " << std::setw(6) << p.rating
                      << " " << std::setw(5) << p.games_played
                      << " " << p.name << "\n";
        }
    }

    void printGameReport() const {
        std::cout << parser_.generateReport();
    }

    void validateAll() const {
        DataValidator::ValidationResult result;
        for (const auto& p : players_) {
            auto r = DataValidator::validateRecord(p);
            if (!r.valid || !r.warnings.empty()) {
                std::cout << "Player " << p.id << " (" << p.name << "): ";
                if (!r.valid) {
                    std::cout << "INVALID";
                    for (const auto& e : r.errors) {
                        std::cout << "\n  Error: " << e;
                    }
                }
                for (const auto& w : r.warnings) {
                    std::cout << "\n  Warning: " << w;
                }
                std::cout << "\n";
            }
        }
    }

private:
    std::vector<PlayerRecord> players_;
    PgnParser parser_;
};

} // namespace cgd


int main(int argc, char* argv[]) {
    if (argc < 2) {
        std::cerr << "Usage: cgd_cli <command> [args]\n\n"
                  << "Commands:\n"
                  << "  --players <file>       Load and display player records\n"
                  << "  --games <file>         Parse and display PGN game data\n"
                  << "  --validate <file>      Validate a database file\n"
                  << "  --report               Generate full report (requires --players + --games)\n";
        return 1;
    }

    cgd::TournamentDataProcessor processor;
    std::string playerFile;
    std::string gameFile;
    bool reportMode = false;

    for (int i = 1; i < argc; ++i) {
        std::string arg = argv[i];
        if (arg == "--players" && i + 1 < argc) {
            playerFile = argv[++i];
        } else if (arg == "--games" && i + 1 < argc) {
            gameFile = argv[++i];
        } else if (arg == "--validate" && i + 1 < argc) {
            auto result = cgd::DataValidator::validateFile(argv[++i]);
            std::cout << "Validation: " << (result.valid ? "PASS" : "FAIL") << "\n";
            std::cout << "Errors: " << result.errorCount
                      << ", Warnings: " << result.warningCount << "\n";
            for (const auto& e : result.errors) {
                std::cout << "  ERROR: " << e << "\n";
            }
            for (const auto& w : result.warnings) {
                std::cout << "  WARN:  " << w << "\n";
            }
            return result.valid ? 0 : 1;
        } else if (arg == "--report") {
            reportMode = true;
        }
    }

    auto start = std::chrono::steady_clock::now();

    if (!playerFile.empty()) {
        processor.loadPlayers(playerFile);
        if (reportMode) {
            processor.printPlayerReport();
        }
    }

    if (!gameFile.empty()) {
        processor.loadGames(gameFile);
        if (reportMode) {
            processor.printGameReport();
        }
    }

    if (!playerFile.empty() && !gameFile.empty()) {
        processor.validateAll();
    }

    auto end = std::chrono::steady_clock::now();
    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    std::cout << "\n(Completed in " << ms << "ms)\n";

    return 0;
}
