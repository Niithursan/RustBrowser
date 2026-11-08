#pragma once
#include "pipe_io.h"

DWORD registry_read_status(const wchar_t* path) {
    HKEY key = nullptr;
    const LSTATUS status = RegOpenKeyExW(HKEY_LOCAL_MACHINE, path, 0, KEY_QUERY_VALUE, &key);
    if (status == ERROR_SUCCESS) RegCloseKey(key);
    return static_cast<DWORD>(status);
}

DWORD system_file_status(const wchar_t* name) {
    wchar_t directory[MAX_PATH]{};
    const UINT length = GetSystemDirectoryW(directory, MAX_PATH);
    require(length > 0 && length < MAX_PATH, "System directory");
    const std::wstring path = std::wstring(directory) + L"\\" + name;
    Handle file(CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr, OPEN_EXISTING, 0, nullptr));
    return file.value == INVALID_HANDLE_VALUE ? GetLastError() : ERROR_SUCCESS;
}

DWORD system_library_status(const wchar_t* name) {
    const HMODULE library = LoadLibraryExW(name, nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!library) return GetLastError();
    FreeLibrary(library);
    return ERROR_SUCCESS;
}

DWORD nsi_service_status(DWORD& state) {
    SC_HANDLE manager = OpenSCManagerW(nullptr, nullptr, SC_MANAGER_CONNECT);
    if (!manager) return GetLastError();
    SC_HANDLE service = OpenServiceW(manager, L"nsi", SERVICE_QUERY_STATUS);
    const DWORD opened = service ? ERROR_SUCCESS : GetLastError();
    DWORD result = opened;
    if (service) {
        SERVICE_STATUS_PROCESS status{};
        DWORD returned = 0;
        if (QueryServiceStatusEx(service, SC_STATUS_PROCESS_INFO,
            reinterpret_cast<BYTE*>(&status), sizeof(status), &returned)) {
            state = status.dwCurrentState;
        } else {
            result = GetLastError();
        }
        CloseServiceHandle(service);
    }
    CloseServiceHandle(manager);
    return result;
}

DWORD child_creation_status(DWORD& created) {
    wchar_t image[32768]{};
    const DWORD length = GetModuleFileNameW(nullptr, image, 32768);
    require(length > 0 && length < 32768, "Owned child image path");
    std::wstring command = L"\"" + std::wstring(image) + L"\" --hold";
    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    startup.dwFlags = STARTF_USESTDHANDLES;
    PROCESS_INFORMATION child{};
    if (!CreateProcessW(image, command.data(), nullptr, nullptr, FALSE,
        CREATE_SUSPENDED | CREATE_NO_WINDOW, nullptr, nullptr, &startup, &child)) {
        return GetLastError();
    }
    created = 1;
    Handle process(child.hProcess), thread(child.hThread);
    require(TerminateProcess(process.value, 78), "Terminate unexpected synthetic descendant");
    require(WaitForSingleObject(process.value, 5000) == WAIT_OBJECT_0, "Unexpected descendant exit");
    return ERROR_SUCCESS;
}

int diagnostic_child(int argc, wchar_t** argv) {
    require(argc == 10, "Fixed diagnostic arguments");
    HANDLE raw_token = nullptr;
    require(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw_token), "Diagnostic child token");
    Handle token(raw_token);
    const auto contained = token_value(token.value, TokenIsAppContainer);
    require(*reinterpret_cast<const DWORD*>(contained.data()) == 1, "Diagnostic AppContainer membership");
    const auto reader = reinterpret_cast<HANDLE>(static_cast<uintptr_t>(std::stoull(argv[2])));
    const auto writer = reinterpret_cast<HANDLE>(static_cast<uintptr_t>(std::stoull(argv[3])));
    require(GetFileType(reader) == FILE_TYPE_PIPE && GetFileType(writer) == FILE_TYPE_PIPE, "IPC handles are pipes");
    const uint64_t session = std::stoull(argv[4]);
    for (uint32_t sequence = 0; sequence < wire::echo_count; ++sequence) {
        const auto request = read_frame(reader);
        require(wire::echo(request, session, wire::Kind::Echo, sequence), "Fixed echo request");
        if (std::wstring(argv[1]) == L"--diagnostic-hang") Sleep(INFINITE);
        auto reply = wire::frame(session, wire::Kind::Reply, sequence, 0, 0, sequence);
        if (std::wstring(argv[1]) == L"--diagnostic-malformed") {
            reply[3] = 0xffffffff; // Actual malformed worker response, bounded to 48 bytes.
            write_frame(writer, reply);
            Sleep(INFINITE);
        }
        write_frame(writer, reply);
    }
    try_signal_sentinel(reinterpret_cast<HANDLE>(static_cast<uintptr_t>(std::stoull(argv[5]))));
    Handle parent(OpenProcess(PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_DUP_HANDLE, FALSE, std::stoul(argv[6])));
    require(!parent.value && GetLastError() == ERROR_ACCESS_DENIED, "Diagnostic parent process denied");
    uint32_t sequence = wire::echo_count;
    auto report = [&](wire::Operation operation, DWORD status, DWORD first = 0, DWORD second = 0) {
        write_frame(writer, wire::frame(session, wire::Kind::Diagnostic, sequence++,
            static_cast<uint32_t>(operation), status, first, second));
    };
    report(wire::Operation::Catalog, registry_read_status(
        L"SYSTEM\\CurrentControlSet\\Services\\WinSock2\\Parameters\\Protocol_Catalog9"));
    report(wire::Operation::NamespaceCatalog, registry_read_status(
        L"SYSTEM\\CurrentControlSet\\Services\\WinSock2\\Parameters\\NameSpace_Catalog5"));
    SC_HANDLE manager = OpenSCManagerW(nullptr, nullptr, SC_MANAGER_CONNECT);
    const DWORD manager_status = manager ? ERROR_SUCCESS : GetLastError();
    if (manager) CloseServiceHandle(manager);
    report(wire::Operation::ServiceManager, manager_status);
    DWORD service_state = 0;
    const DWORD service_status = nsi_service_status(service_state);
    report(wire::Operation::NsiService, service_status, service_state);
    report(wire::Operation::Ws2Read, system_file_status(L"ws2_32.dll"));
    report(wire::Operation::ProviderRead, system_file_status(L"mswsock.dll"));
    report(wire::Operation::ProviderLoad, system_library_status(L"mswsock.dll"));
    report(wire::Operation::OpenGlRead, system_file_status(L"opengl32.dll"));
    SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
    report(wire::Operation::OpenGlLoad, system_library_status(L"opengl32.dll"));
    WSADATA wsa{};
    const int startup = WSAStartup(MAKEWORD(2, 2), &wsa);
    report(wire::Operation::WinsockStartup, static_cast<DWORD>(startup));
    DWORD created = 0;
    const DWORD creation_status = child_creation_status(created);
    report(wire::Operation::ChildCreation, creation_status, created);
    void* allocation = VirtualAlloc(nullptr, 128ULL * 1024 * 1024, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    const DWORD allocation_status = allocation ? ERROR_SUCCESS : GetLastError();
    if (allocation) VirtualFree(allocation, 0, MEM_RELEASE);
    report(wire::Operation::Allocation, allocation_status, allocation ? 1 : 0);
    DWORD udp_status = ERROR_NOT_SUPPORTED;
    DWORD udp_sent = 0;
    if (startup == 0) {
        SOCKET socket_handle = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
        if (socket_handle == INVALID_SOCKET) {
            udp_status = static_cast<DWORD>(WSAGetLastError());
        } else {
            sockaddr_in target{};
            target.sin_family = AF_INET;
            target.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
            target.sin_port = htons(static_cast<u_short>(std::stoul(argv[7])));
            const int sent = sendto(socket_handle, reinterpret_cast<const char*>(&session), sizeof(session),
                0, reinterpret_cast<sockaddr*>(&target), sizeof(target));
            udp_status = sent == SOCKET_ERROR ? static_cast<DWORD>(WSAGetLastError()) : ERROR_SUCCESS;
            udp_sent = sent == SOCKET_ERROR ? 0 : static_cast<DWORD>(sent);
            closesocket(socket_handle);
        }
        WSACleanup();
    }
    report(wire::Operation::UdpSend, udp_status, udp_sent);
    const auto elevation = token_value(token.value, TokenElevation);
    report(wire::Operation::TokenElevation, ERROR_SUCCESS,
        reinterpret_cast<const TOKEN_ELEVATION*>(elevation.data())->TokenIsElevated);
    const auto ui_access = token_value(token.value, TokenUIAccess);
    report(wire::Operation::TokenUiAccess, ERROR_SUCCESS,
        *reinterpret_cast<const DWORD*>(ui_access.data()));
    const auto privileges = token_value(token.value, TokenPrivileges);
    const auto* privilege_list = reinterpret_cast<const TOKEN_PRIVILEGES*>(privileges.data());
    DWORD enabled = 0;
    for (DWORD index = 0; index < privilege_list->PrivilegeCount; ++index) {
        if (privilege_list->Privileges[index].Attributes & SE_PRIVILEGE_ENABLED) ++enabled;
    }
    report(wire::Operation::EnabledPrivileges, ERROR_SUCCESS, enabled);
    auto file_probe = [](const wchar_t* path, DWORD access) {
        Handle file(CreateFileW(path, access, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr));
        return file.value == INVALID_HANDLE_VALUE ? GetLastError() : ERROR_SUCCESS;
    };
    report(wire::Operation::AppPackagesRead, file_probe(argv[8], GENERIC_READ));
    report(wire::Operation::PrivateRead, file_probe(argv[9], GENERIC_READ));
    report(wire::Operation::PrivateWrite, file_probe(argv[9], GENERIC_WRITE));
    write_frame(writer, wire::frame(session, wire::Kind::Complete, sequence));
    return 0;
}
