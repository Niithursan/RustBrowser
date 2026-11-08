#pragma once
#include "pipe_io.h"
#include <bcrypt.h>
#include <algorithm>
#pragma comment(lib, "bcrypt.lib")

struct Channel {
    Handle parent_read, child_write, child_read, parent_write;
    Channel() {
        SECURITY_ATTRIBUTES security{sizeof(SECURITY_ATTRIBUTES), nullptr, TRUE};
        require(CreatePipe(&parent_read.value, &child_write.value, &security, 4096), "Create report pipe");
        require(CreatePipe(&child_read.value, &parent_write.value, &security, 4096), "Create request pipe");
        require(SetHandleInformation(parent_read.value, HANDLE_FLAG_INHERIT, 0), "Protect parent read endpoint");
        require(SetHandleInformation(parent_write.value, HANDLE_FLAG_INHERIT, 0), "Protect parent write endpoint");
    }
    void release_child_endpoints() {
        require(CloseHandle(child_read.value), "Release broker copy of child read endpoint");
        child_read.value = nullptr;
        require(CloseHandle(child_write.value), "Release broker copy of child write endpoint");
        child_write.value = nullptr;
    }
};

struct ProtocolFailure : std::runtime_error {
    explicit ProtocolFailure(const char* reason) : std::runtime_error(reason) {}
};

void expect_wire(bool valid, const char* reason) {
    if (!valid) throw ProtocolFailure(reason);
}

double milliseconds(LARGE_INTEGER start, LARGE_INTEGER end, LARGE_INTEGER frequency) {
    return 1000.0 * static_cast<double>(end.QuadPart - start.QuadPart) / static_cast<double>(frequency.QuadPart);
}

void run_diagnostics(const fs::path& exe, const fs::path& run, PSID sid, HANDLE job,
    HANDLE sentinel, bool lpac, const std::wstring& mode) {
    Channel channel;
    uint64_t session = 0;
    require(BCryptGenRandom(nullptr, reinterpret_cast<PUCHAR>(&session), sizeof(session),
        BCRYPT_USE_SYSTEM_PREFERRED_RNG) == 0, "Generate IPC instance identifier");
    SOCKET udp = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    require(udp != INVALID_SOCKET, "UDP positive-control endpoint");
    sockaddr_in address{};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    require(bind(udp, reinterpret_cast<sockaddr*>(&address), sizeof(address)) == 0, "Bind owned UDP endpoint");
    int address_size = sizeof(address);
    require(getsockname(udp, reinterpret_cast<sockaddr*>(&address), &address_size) == 0, "UDP port");
    SOCKET control = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    require(control != INVALID_SOCKET, "UDP positive-control sender");
    require(sendto(control, reinterpret_cast<const char*>(&session), sizeof(session), 0,
        reinterpret_cast<sockaddr*>(&address), sizeof(address)) == static_cast<int>(sizeof(session)), "UDP positive send");
    fd_set readable{};
    FD_SET(udp, &readable);
    timeval wait{1, 0};
    require(select(0, &readable, nullptr, nullptr, &wait) == 1, "UDP positive receive ready");
    uint64_t received_control = 0;
    require(recv(udp, reinterpret_cast<char*>(&received_control), sizeof(received_control), 0) == static_cast<int>(sizeof(session))
        && received_control == session, "UDP positive payload");
    closesocket(control);

    const std::wstring child_mode = mode == L"malformed" ? L"--diagnostic-malformed" :
        mode == L"hang" ? L"--diagnostic-hang" : L"--diagnostic-child";
    const std::wstring args = child_mode + L" " +
        std::to_wstring(reinterpret_cast<uintptr_t>(channel.child_read.value)) + L" " +
        std::to_wstring(reinterpret_cast<uintptr_t>(channel.child_write.value)) + L" " +
        std::to_wstring(session) + L" " + std::to_wstring(reinterpret_cast<uintptr_t>(sentinel)) + L" " +
        std::to_wstring(GetCurrentProcessId()) + L" " + std::to_wstring(ntohs(address.sin_port)) +
        L" \"" + (run / L"apps-readable.txt").wstring() + L"\" \"" + (run / L"private-sentinel.txt").wstring() + L"\"";
    LARGE_INTEGER frequency{}, started{}, resumed{}, first_reply{}, finished{};
    require(QueryPerformanceFrequency(&frequency) && QueryPerformanceCounter(&started), "Monotonic probe clock");
    const auto child_info = launch(exe, args, run, sid, job, lpac,
        {channel.child_read.value, channel.child_write.value});
    Child child(child_info);
    verify_token(child.process.value, sid);
    BOOL in_job = FALSE;
    require(IsProcessInJob(child.process.value, job, &in_job) && in_job, "Creation-time job membership");
    channel.release_child_endpoints();
    require(QueryPerformanceCounter(&resumed), "Resume clock");
    require(ResumeThread(child.thread.value) != static_cast<DWORD>(-1), "Resume diagnostic worker");
    std::vector<double> roundtrips;
    try {
        for (uint32_t sequence = 0; sequence < wire::echo_count; ++sequence) {
            LARGE_INTEGER before{}, after{};
            require(QueryPerformanceCounter(&before), "IPC start clock");
            write_frame(channel.parent_write.value, wire::frame(session, wire::Kind::Echo, sequence, 0, 0, sequence));
            const auto reply = read_frame(channel.parent_read.value, mode == L"hang" ? 200 : 5000, child.process.value);
            expect_wire(wire::echo(reply, session, wire::Kind::Reply, sequence), "Invalid worker reply rejected");
            require(QueryPerformanceCounter(&after), "IPC end clock");
            if (sequence == 0) first_reply = after;
            roundtrips.push_back(milliseconds(before, after, frequency));
            if (mode == L"crash" || mode == L"crash-lpac") {
                record_child_identity(child_info.dwProcessId, child.process.value, run);
                TerminateProcess(GetCurrentProcess(), 74);
                throw std::runtime_error("Intentional broker termination failed");
            }
        }
        if (mode == L"malformed" || mode == L"hang") {
            throw std::runtime_error("Fault fixture unexpectedly completed the valid protocol");
        }
    } catch (const ProtocolFailure&) {
        if (mode != L"malformed") throw;
        LARGE_INTEGER shutdown_start{}, shutdown_end{};
        require(QueryPerformanceCounter(&shutdown_start), "Malformed shutdown start");
        require(TerminateJobObject(job, 76), "Terminate malformed worker");
        require(child.wait() == 76, "Malformed worker exited");
        require(QueryPerformanceCounter(&shutdown_end), "Malformed shutdown end");
        closesocket(udp);
        std::cout << "malformed_worker_reply_rejected=passed\n";
        std::cout << "malformed_shutdown_ms=" << milliseconds(shutdown_start, shutdown_end, frequency) << "\n";
        return;
    } catch (const WinFailure& error) {
        if (mode != L"hang" || error.code != ERROR_TIMEOUT) throw;
        LARGE_INTEGER shutdown_start{}, shutdown_end{};
        require(QueryPerformanceCounter(&shutdown_start), "Hang shutdown start");
        require(TerminateJobObject(job, 75), "Terminate hung worker");
        require(child.wait() == 75, "Hung worker exited");
        require(QueryPerformanceCounter(&shutdown_end), "Hang shutdown end");
        closesocket(udp);
        std::cout << "hung_worker_deadline_and_exit=passed\n";
        std::cout << "hang_shutdown_ms=" << milliseconds(shutdown_start, shutdown_end, frequency) << "\n";
        return;
    }
    std::array<wire::Frame, wire::max_diagnostics> reports{};
    uint32_t sequence = wire::echo_count;
    for (uint32_t operation = 1; operation < static_cast<uint32_t>(wire::Operation::Last); ++operation) {
        const auto report = read_frame(channel.parent_read.value, 5000, child.process.value);
        expect_wire(wire::diagnostic(report, session, sequence++) && report[8] == operation,
            "Unexpected, replayed or duplicate diagnostic rejected");
        reports[operation] = report;
        std::cout << "operation=" << operation << " status=" << report[9] <<
            " value=" << report[10] << " extra=" << report[11] << "\n";
    }
    const auto complete = read_frame(channel.parent_read.value, 5000, child.process.value);
    expect_wire(wire::header(complete, session, wire::Kind::Complete, sequence) &&
        complete[8] == 0 && complete[9] == 0 && complete[10] == 0 && complete[11] == 0, "Bounded completion frame");
    require(child.wait() == 0, "Diagnostic worker exit");
    require(QueryPerformanceCounter(&finished), "Exit clock");
    require(WaitForSingleObject(sentinel, 0) == WAIT_TIMEOUT, "Unlisted sentinel handle was not inherited");
    const auto file_status = [&](wire::Operation operation) {
        return reports[static_cast<uint32_t>(operation)][9];
    };
    const DWORD expected_file_status = lpac ? ERROR_ACCESS_DENIED : ERROR_SUCCESS;
    require(file_status(wire::Operation::AppPackagesRead) == expected_file_status,
        "AppContainer / LPAC file positive and negative control");
    require(file_status(wire::Operation::PrivateRead) == ERROR_ACCESS_DENIED &&
        file_status(wire::Operation::PrivateWrite) == ERROR_ACCESS_DENIED, "Private file controls with IPC enabled");
    const auto& creation = reports[static_cast<uint32_t>(wire::Operation::ChildCreation)];
    require(creation[10] == 0 && (creation[9] == ERROR_ACCESS_DENIED ||
        creation[9] == ERROR_CHILD_PROCESS_BLOCKED || creation[9] == ERROR_NOT_ENOUGH_QUOTA), "Child creation actually denied");
    const auto& allocation = reports[static_cast<uint32_t>(wire::Operation::Allocation)];
    require(allocation[10] == 0 && allocation[11] == 1 && (allocation[9] == ERROR_NOT_ENOUGH_MEMORY ||
        allocation[9] == ERROR_COMMITMENT_LIMIT), "Allocation exceeded the measured job limit");
    FD_ZERO(&readable);
    FD_SET(udp, &readable);
    wait = {0, 200000};
    const int ready = select(0, &readable, nullptr, nullptr, &wait);
    require(ready == 0, "No diagnostic worker datagram observed within 200 ms");
    closesocket(udp);
    FILETIME creation_time{}, exit_time{}, kernel_time{}, user_time{};
    require(GetProcessTimes(child.process.value, &creation_time, &exit_time, &kernel_time, &user_time), "Worker CPU accounting");
    auto ticks = [](FILETIME value) {
        ULARGE_INTEGER number{};
        number.LowPart = value.dwLowDateTime;
        number.HighPart = value.dwHighDateTime;
        return number.QuadPart;
    };
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
    require(QueryInformationJobObject(job, JobObjectExtendedLimitInformation, &limits, sizeof(limits), nullptr),
        "Worker resource accounting");
    require(limits.ProcessMemoryLimit == 64ULL * 1024 * 1024 &&
        (limits.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_PROCESS_MEMORY),
        "Measured worker memory limit is 64 MiB");
    auto sorted_roundtrips = roundtrips;
    std::sort(sorted_roundtrips.begin(), sorted_roundtrips.end());
    const size_t middle = sorted_roundtrips.size() / 2;
    std::cout << "worker_mode=" << (lpac ? "lpac" : "appcontainer") <<
        " startup_first_reply_ms=" << milliseconds(started, first_reply, frequency) <<
        " resume_to_exit_ms=" << milliseconds(resumed, finished, frequency) <<
        " cpu_ms=" << static_cast<double>(ticks(kernel_time) + ticks(user_time)) / 10000.0 <<
        " peak_private_mib=" << static_cast<double>(limits.PeakProcessMemoryUsed) / (1024.0 * 1024.0) <<
        " ipc_median_ms=" << (sorted_roundtrips[middle - 1] + sorted_roundtrips[middle]) / 2.0 << "\n";
    for (const double value : roundtrips) std::cout << "ipc_roundtrip_ms=" << value << "\n";
    std::cout << "diagnostic_ipc_and_limits=passed\n";
}
