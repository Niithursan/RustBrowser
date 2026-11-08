#include "probe_windows.h"
int child_stage = 10;
void try_signal_sentinel(HANDLE handle) {
    // Windows can raise STATUS_INVALID_HANDLE under handle checking.
    // Handle-number reuse is harmless unless it grants access to the parent's event.
    __try { SetEvent(handle); }
    __except (GetExceptionCode() == 0xC0000008UL ? EXCEPTION_EXECUTE_HANDLER : EXCEPTION_CONTINUE_SEARCH) {}
}
int child_probe(int argc, wchar_t** argv) {
    child_stage = 10;
    require(argc == 7, "Probe argument count");
    HANDLE token = nullptr;
    require(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token) != FALSE, "Self token");
    Handle own_token(token);
    const auto data = token_value(token, TokenIsAppContainer);
    require(*reinterpret_cast<const DWORD*>(data.data()) == 1, "Child is contained");
    const DWORD access_modes[] = {GENERIC_READ, GENERIC_WRITE};
    for (const DWORD access : access_modes) {
        child_stage = access == GENERIC_READ ? 20 : 21;
        Handle file(CreateFileW(argv[2], access, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr));
        require(file.value == INVALID_HANDLE_VALUE && GetLastError() == ERROR_ACCESS_DENIED, "Private sentinel denied");
    }
    child_stage = 30;
    Handle apps_file(CreateFileW(argv[6], GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr));
    if (std::wstring(argv[1]) == L"--lpac") {
        require(apps_file.value == INVALID_HANDLE_VALUE && GetLastError() == ERROR_ACCESS_DENIED,
            "ALL APPLICATION PACKAGES read grant does not admit LPAC");
    } else {
        require(std::wstring(argv[1]) == L"--appcontainer" && apps_file.value != INVALID_HANDLE_VALUE,
            "Standard AppContainer admits explicit ALL APPLICATION PACKAGES read grant");
    }
    child_stage = 40;
    Handle parent(OpenProcess(PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_DUP_HANDLE, FALSE, std::stoul(argv[3])));
    require(!parent.value && GetLastError() == ERROR_ACCESS_DENIED, "Parent process denied");
    child_stage = 50;
    const HANDLE inherited = reinterpret_cast<HANDLE>(static_cast<uintptr_t>(std::stoull(argv[4])));
    // A numeric handle can be reused for an unrelated object in the child.
    // The parent verifies that this attempt cannot signal its sentinel event.
    try_signal_sentinel(inherited);
    child_stage = 60;
    WSADATA wsa{};
    const int wsa_start = WSAStartup(MAKEWORD(2, 2), &wsa);
    child_stage = 60000 + wsa_start;
    require(wsa_start == 0, "Child Winsock initialization");
    child_stage = 61;
    SOCKET connection = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    require(connection != INVALID_SOCKET, "Child socket creation");
    sockaddr_in address{};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = htons(static_cast<u_short>(std::stoul(argv[5])));
    u_long nonblocking = 1;
    require(ioctlsocket(connection, FIONBIO, &nonblocking) == 0, "Bounded child socket");
    child_stage = 70;
    const int connected = connect(connection, reinterpret_cast<sockaddr*>(&address), sizeof(address));
    int error = WSAGetLastError();
    if (connected == SOCKET_ERROR && error == WSAEWOULDBLOCK) {
        fd_set writable{};
        fd_set failed{};
        FD_SET(connection, &writable);
        FD_SET(connection, &failed);
        timeval timeout{2, 0};
        const int decision = select(0, nullptr, &writable, &failed, &timeout);
        require(decision != SOCKET_ERROR, "Bounded connection observation");
        if (decision == 0) {
            closesocket(connection);
            WSACleanup();
            return 2; // No connection observed. A timeout is not an explicit policy denial.
        }
        int error_size = sizeof(error);
        require(getsockopt(connection, SOL_SOCKET, SO_ERROR, reinterpret_cast<char*>(&error), &error_size) == 0,
            "Read connection decision");
    }
    closesocket(connection);
    WSACleanup();
    child_stage = 100000 + error;
    require(connected == SOCKET_ERROR && error == WSAEACCES, "Loopback connect denied by policy");
    return 0;
}

#include "diagnostics_child.h"

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
    const int argc = __argc;
    wchar_t** argv = __wargv;
    try {
        if (argc > 1 && std::wstring(argv[1]).find(L"--diagnostic-") == 0) {
            return diagnostic_child(argc, argv);
        }
        if (argc == 2 && std::wstring(argv[1]) == L"--hold") {
            HANDLE raw = nullptr;
            require(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw) != FALSE, "Self token");
            Handle token(raw);
            const auto data = token_value(token.value, TokenIsAppContainer);
            require(*reinterpret_cast<const DWORD*>(data.data()) == 1, "Contained hold child");
            Sleep(INFINITE);
        }
        return child_probe(argc, argv);
    }
    catch (const std::exception&) { return child_stage; }
}
