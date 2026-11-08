#pragma once
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <winsock2.h>
#include <windows.h>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>
#pragma comment(lib, "advapi32.lib")
#pragma comment(lib, "ws2_32.lib")
struct WinFailure : std::runtime_error {
    DWORD code;
    WinFailure(const char* operation, DWORD error) :
        std::runtime_error(std::string(operation) + " error=" + std::to_string(error)), code(error) {}
};

void require(bool condition, const char* operation) {
    if (!condition) {
        throw WinFailure(operation, GetLastError());
    }
}

struct Handle {
    HANDLE value = nullptr;
    explicit Handle(HANDLE handle = nullptr) : value(handle) {}
    ~Handle() { if (value && value != INVALID_HANDLE_VALUE) CloseHandle(value); }
    Handle(const Handle&) = delete;
    Handle& operator=(const Handle&) = delete;
};

std::vector<BYTE> token_value(HANDLE token, TOKEN_INFORMATION_CLASS kind) {
    DWORD size = 0;
    GetTokenInformation(token, kind, nullptr, 0, &size);
    const auto operation = "Token size class=" + std::to_string(kind);
    require(size > 0, operation.c_str());
    std::vector<BYTE> value(size);
    require(GetTokenInformation(token, kind, value.data(), size, &size) != FALSE, "Token information");
    return value;
}
