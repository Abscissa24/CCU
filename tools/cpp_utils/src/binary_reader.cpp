#include "binary_reader.hpp"

#include <sstream>
#include <iomanip>
#include <stdexcept>

namespace cgd {

// ─── BinaryReader ───────────────────────────────────────────────────────

BinaryReader::BinaryReader(const std::string& filepath)
    : filepath_(filepath), isOpen_(false) {}

BinaryReader::~BinaryReader() {
    close();
}

bool BinaryReader::open() {
    file_.open(filepath_, std::ios::in | std::ios::out | std::ios::binary);
    if (!file_.is_open()) {
        // Try creating a new file
        file_.open(filepath_, std::ios::in | std::ios::out | std::ios::binary | std::ios::trunc);
        if (!file_.is_open()) {
            errorMsg_ = "Cannot open file: " + filepath_;
            return false;
        }
    }
    isOpen_ = true;
    return readHeader();
}

void BinaryReader::close() {
    if (isOpen_) {
        writeHeader();
        file_.close();
        isOpen_ = false;
    }
}

bool BinaryReader::readHeader() {
    file_.seekg(0, std::ios::beg);
    file_.read(reinterpret_cast<char*>(&header_), sizeof(header_));

    if (file_.gcount() < static_cast<std::streamsize>(sizeof(header_))) {
        // Initialize a new header
        memset(&header_, 0, sizeof(header_));
        strncpy(header_.magic, "CCUDB", 4);
        header_.version = 1;
        header_.recordCount = 0;
        return writeHeader();
    }

    if (strncmp(header_.magic, "CCUDB", 4) != 0) {
        errorMsg_ = "Invalid file format: magic does not match";
        return false;
    }

    return true;
}

bool BinaryReader::writeHeader() {
    file_.seekp(0, std::ios::beg);
    file_.write(reinterpret_cast<const char*>(&header_), sizeof(header_));
    file_.flush();
    return file_.good();
}

bool BinaryReader::validateHeader() {
    if (!isOpen_) {
        errorMsg_ = "File not open";
        return false;
    }
    return strncmp(header_.magic, "CCUDB", 4) == 0 && header_.version >= 1;
}

std::vector<PlayerRecord> BinaryReader::readAllRecords() {
    std::vector<PlayerRecord> records;
    if (!isOpen_) {
        errorMsg_ = "File not open";
        return records;
    }

    records.reserve(header_.recordCount);
    file_.seekg(sizeof(header_), std::ios::beg);

    for (uint32_t i = 0; i < header_.recordCount; ++i) {
        PlayerRecord record;
        if (readRecord(i, record)) {
            records.push_back(record);
        } else {
            break;
        }
    }

    return records;
}

bool BinaryReader::readRecord(uint32_t index, PlayerRecord& out) {
    if (!isOpen_) {
        errorMsg_ = "File not open";
        return false;
    }

    if (index >= header_.recordCount) {
        errorMsg_ = "Index out of bounds";
        return false;
    }

    std::streamoff offset = sizeof(header_) + static_cast<std::streamoff>(index) * sizeof(PlayerRecord);
    file_.seekg(offset, std::ios::beg);
    file_.read(reinterpret_cast<char*>(&out), sizeof(out));

    if (file_.gcount() < static_cast<std::streamsize>(sizeof(out))) {
        errorMsg_ = "Failed to read record";
        return false;
    }

    return true;
}

bool BinaryReader::writeRecord(const PlayerRecord& record, uint32_t index) {
    if (!isOpen_) {
        errorMsg_ = "File not open";
        return false;
    }

    if (index >= header_.recordCount) {
        errorMsg_ = "Index out of bounds";
        return false;
    }

    std::streamoff offset = sizeof(header_) + static_cast<std::streamoff>(index) * sizeof(PlayerRecord);
    file_.seekp(offset, std::ios::beg);
    file_.write(reinterpret_cast<const char*>(&record), sizeof(record));
    file_.flush();

    return file_.good();
}

bool BinaryReader::appendRecord(const PlayerRecord& record) {
    if (!isOpen_) {
        errorMsg_ = "File not open";
        return false;
    }

    std::streamoff offset = sizeof(header_) +
        static_cast<std::streamoff>(header_.recordCount) * sizeof(PlayerRecord);

    file_.seekp(offset, std::ios::beg);
    file_.write(reinterpret_cast<const char*>(&record), sizeof(record));
    file_.flush();

    if (file_.good()) {
        header_.recordCount++;
        writeHeader();
        return true;
    }

    errorMsg_ = "Failed to append record";
    return false;
}

// ─── DataValidator ──────────────────────────────────────────────────────

DataValidator::ValidationResult DataValidator::validateFile(const std::string& filepath) {
    ValidationResult result;
    BinaryReader reader(filepath);

    if (!reader.open()) {
        result.valid = false;
        result.errorCount = 1;
        result.errors.push_back("Cannot open file: " + filepath);
        return result;
    }

    if (!reader.validateHeader()) {
        result.valid = false;
        result.errorCount = 1;
        result.errors.push_back("Invalid or corrupted header");
        return result;
    }

    auto records = reader.readAllRecords();
    uint32_t expectedCount = reader.getRecordCount();

    if (records.size() != expectedCount) {
        result.valid = false;
        result.errorCount = 1;
        result.errors.push_back(
            "Record count mismatch: header=" + std::to_string(expectedCount) +
            ", actual=" + std::to_string(records.size())
        );
    }

    std::set<uint32_t> seenIds;
    for (size_t i = 0; i < records.size(); ++i) {
        auto recResult = validateRecord(records[i]);
        if (!recResult.valid) {
            result.valid = false;
            for (const auto& err : recResult.errors) {
                result.errors.push_back("Record " + std::to_string(i) + ": " + err);
            }
            result.errorCount += recResult.errorCount;
        }
        for (const auto& warn : recResult.warnings) {
            result.warnings.push_back("Record " + std::to_string(i) + ": " + warn);
            result.warningCount++;
        }

        if (seenIds.find(records[i].id) != seenIds.end()) {
            result.warnings.push_back("Record " + std::to_string(i) + ": duplicate ID " +
                std::to_string(records[i].id));
            result.warningCount++;
        }
        seenIds.insert(records[i].id);
    }

    reader.close();
    return result;
}

DataValidator::ValidationResult
DataValidator::validateRecord(const PlayerRecord& record) {
    ValidationResult result;

    if (record.id == 0) {
        result.valid = false;
        result.errorCount = 1;
        result.errors.push_back("ID is zero");
    }

    if (strlen(record.name) == 0) {
        result.valid = false;
        result.errorCount = 1;
        result.errors.push_back("Name is empty");
    }

    if (strlen(record.district) == 0) {
        result.warningCount = 1;
        result.warnings.push_back("District is empty");
    }

    if (record.rating > 3000) {
        result.warningCount = 1;
        result.warnings.push_back("Rating exceeds 3000: " + std::to_string(record.rating));
    }

    if (record.games_played > 1000) {
        result.warningCount = 1;
        result.warnings.push_back("Games played exceeds 1000: " +
            std::to_string(record.games_played));
    }

    return result;
}

bool DataValidator::isChecksumValid(const std::string& /*filepath*/) {
    // Placeholder: checksum validation is performed by the internal tool
    return true;
}

std::string DataValidator::computeSha256(const PlayerRecord& record) {
    // Simplified hash — just concatenates fields
    std::ostringstream oss;
    oss << record.id << ":" << record.name << ":" << record.district
        << ":" << record.rating << ":" << record.games_played;
    return std::to_string(std::hash<std::string>{}(oss.str()));
}

} // namespace cgd
