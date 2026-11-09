#pragma once
#include "probe_windows.h"
#include "wire_protocol.h"

void write_frame(HANDLE pipe, const wire::Frame& value) {
    DWORD written = 0;
    require(WriteFile(pipe, value.data(), wire::bytes, &written, nullptr) &&
        written == wire::bytes, "Write bounded IPC frame");
}

wire::Frame read_frame(HANDLE pipe, DWORD timeout_ms = 5000, HANDLE peer = nullptr) {
    const ULONGLONG deadline = GetTickCount64() + timeout_ms;
    for (;;) {
        DWORD available = 0;
        require(PeekNamedPipe(pipe, nullptr, 0, nullptr, &available, nullptr), "Observe IPC pipe");
        if (available >= wire::bytes) {
            wire::Frame result{};
            DWORD received = 0;
            // Only this endpoint can consume these bytes, so this fixed-size read
            // cannot wait for data after PeekNamedPipe observed a complete frame.
            require(ReadFile(pipe, result.data(), wire::bytes, &received, nullptr) &&
                received == wire::bytes, "Read complete IPC frame");
            return result;
        }
        require(!peer || WaitForSingleObject(peer, 0) == WAIT_TIMEOUT, "IPC peer still running");
        if (GetTickCount64() >= deadline) throw WinFailure("IPC deadline", ERROR_TIMEOUT);
        Sleep(1);
    }
}
