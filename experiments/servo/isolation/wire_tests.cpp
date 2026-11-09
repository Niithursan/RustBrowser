#include "wire_protocol.h"
#include <iostream>
#include <stdexcept>

void check(bool value) {
    if (!value) throw std::runtime_error("Wire protocol regression");
}

int main() {
    try {
        constexpr uint64_t session = 0x123456789abcdef0;
        const auto request = wire::frame(session, wire::Kind::Echo, 7, 0, 0, 7);
        check(wire::echo(request, session, wire::Kind::Echo, 7));
        for (const size_t field : {0u, 1u, 2u, 3u, 4u, 5u, 6u, 7u, 8u, 9u, 10u, 11u}) {
            auto bad = request;
            bad[field] ^= 0xffffffff;
            check(!wire::echo(bad, session, wire::Kind::Echo, 7));
        }
        check(!wire::echo(request, session + 1, wire::Kind::Echo, 7));
        check(!wire::echo(request, session, wire::Kind::Echo, 8));
        check(!wire::echo(request, session, wire::Kind::Reply, 7));
        const auto report = wire::frame(session, wire::Kind::Diagnostic, 16,
            static_cast<uint32_t>(wire::Operation::WinsockStartup), 10107);
        check(wire::diagnostic(report, session, 16));
        auto bad = report;
        bad[8] = static_cast<uint32_t>(wire::Operation::Last);
        check(!wire::diagnostic(bad, session, 16));
        bad = report;
        bad[3] = 0xffffffff;
        check(!wire::diagnostic(bad, session, 16));
        std::cout << "wire_protocol_mutation_replay_origin_size_and_opcode_tests=passed\n";
        return 0;
    } catch (const std::exception& error) {
        std::cerr << error.what() << "\n";
        return 1;
    }
}
