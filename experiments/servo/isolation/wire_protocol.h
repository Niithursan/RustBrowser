#pragma once
#include <array>
#include <cstdint>

// Fixed-size little-endian words for Windows x64. No pointers, strings, paths,
// handles, or allocations are accepted from the worker.
namespace wire {
using Frame = std::array<uint32_t, 12>;
constexpr uint32_t magic = 0x52425031;
constexpr uint32_t version = 1;
constexpr uint32_t bytes = 48;
constexpr uint32_t fixture_origin = 1;
constexpr uint32_t echo_count = 16;
constexpr uint32_t max_diagnostics = 32;
static_assert(sizeof(Frame) == bytes);

enum class Kind : uint32_t { Echo = 1, Reply = 2, Diagnostic = 3, Complete = 4 };
enum class Operation : uint32_t {
    Catalog = 1, NamespaceCatalog, ServiceManager, NsiService,
    Ws2Read, ProviderRead, ProviderLoad, OpenGlRead, OpenGlLoad,
    WinsockStartup, ChildCreation, Allocation, UdpSend, TokenElevation,
    TokenUiAccess, EnabledPrivileges, AppPackagesRead, PrivateRead, PrivateWrite, Last
};

Frame frame(uint64_t session, Kind kind, uint32_t sequence,
    uint32_t operation = 0, uint32_t status = 0, uint32_t first = 0, uint32_t second = 0) {
    return {magic, version, static_cast<uint32_t>(kind), bytes, sequence, fixture_origin,
        static_cast<uint32_t>(session), static_cast<uint32_t>(session >> 32),
        operation, status, first, second};
}

bool header(const Frame& value, uint64_t session, Kind kind, uint32_t sequence) {
    return value[0] == magic && value[1] == version && value[2] == static_cast<uint32_t>(kind) &&
        value[3] == bytes && value[4] == sequence && value[5] == fixture_origin &&
        value[6] == static_cast<uint32_t>(session) && value[7] == static_cast<uint32_t>(session >> 32);
}

bool echo(const Frame& value, uint64_t session, Kind kind, uint32_t sequence) {
    return header(value, session, kind, sequence) && value[8] == 0 &&
        value[9] == 0 && value[10] == sequence && value[11] == 0;
}

bool diagnostic(const Frame& value, uint64_t session, uint32_t sequence) {
    return header(value, session, Kind::Diagnostic, sequence) &&
        value[8] >= static_cast<uint32_t>(Operation::Catalog) &&
        value[8] < static_cast<uint32_t>(Operation::Last);
}
}
