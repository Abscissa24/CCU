#pragma once

#include <cstdint>
#include <string>
#include <vector>
#include <fstream>
#include <cstring>

namespace cgd {

#pragma pack(push, 1)
struct FileHeader {
    char magic[4];      // "CCUDB"
    uint32_t version;
    uint32_t recordCount;
    uint32_t reserved;
};
#pragma pack(pop)

#pragma pack(push, 1)
struct PlayerRecord {
    uint32_t id;
    char name[64];
    char district[16];
    uint16_t rating;
    uint16_t games_played;
    uint8_t  active;
    uint8_t  padding[3];
};
#pragma pack(pop)

static_assert(sizeof(FileHeader) == 16, "FileHeader must be 16 bytes");
static_assert(sizeof(PlayerRecord) == 80, "PlayerRecord must be 80 bytes");

class BinaryReader {
public:
    explicit BinaryReader(const std::string& filepath);
    ~BinaryReader();

    bool open();
    void close();
    bool isOpen() const { return isOpen_; }

    bool validateHeader();
    uint32_t getVersion() const { return header_.version; }
    uint32_t getRecordCount() const { return header_.recordCount; }

    std::vector<PlayerRecord> readAllRecords();
    bool readRecord(uint32_t index, PlayerRecord& out);
    bool writeRecord(const PlayerRecord& record, uint32_t index);
    bool appendRecord(const PlayerRecord& record);

    std::string getError() const { return errorMsg_; }

private:
    std::string filepath_;
    std::fstream file_;
    FileHeader header_;
    bool isOpen_ = false;
    std::string errorMsg_;

    bool readHeader();
    bool writeHeader();
};

class DataValidator {
public:
    struct ValidationResult {
        bool valid = true;
        int errorCount = 0;
        int warningCount = 0;
        std::vector<std::string> errors;
        std::vector<std::string> warnings;
    };

    static ValidationResult validateFile(const std::string& filepath);
    static ValidationResult validateRecord(const PlayerRecord& record);

    static bool isChecksumValid(const std::string& filepath);
    static std::string computeSha256(const PlayerRecord& record);
};

} // namespace cgd
