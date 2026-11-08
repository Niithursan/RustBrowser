#include "probe_windows.h"
#include <userenv.h>
#include <aclapi.h>
#include <filesystem>
#include <iostream>
#pragma comment(lib, "userenv.lib")
namespace fs = std::filesystem;
struct Container {
    std::wstring name;
    PSID sid = nullptr;
    bool created = false;
    fs::path journal;
    explicit Container(std::wstring moniker) : name(std::move(moniker)) {
        const HRESULT result = CreateAppContainerProfile(name.c_str(), L"Rust Browser synthetic probe",
            L"No web content or network capabilities", nullptr, 0, &sid);
        if (result != S_OK) {
            throw std::runtime_error("CreateAppContainerProfile HRESULT=" +
                std::to_string(static_cast<uint32_t>(result)));
        }
        created = true;
    }
    bool remove() {
        if (!created) return true;
        const bool ok = SUCCEEDED(DeleteAppContainerProfile(name.c_str()));
        if (ok) {
            created = false;
            if (!journal.empty()) require(DeleteFileW(journal.c_str()) != FALSE, "Remove owned profile journal");
        }
        return ok;
    }
    void record_creation(const fs::path& run) {
        const auto candidate = run / L"profile-created.marker";
        Handle file(CreateFileW(candidate.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_NEW, 0, nullptr));
        require(file.value != INVALID_HANDLE_VALUE, "Reserve owned profile journal");
        journal = candidate;
    }
    ~Container() {
        if (created) {
            // Destructors must report cleanup failures without throwing.
            try { std::cerr << (remove() ? "owned_profile_removed_after_failure\n" : "profile_cleanup_failed\n"); }
            catch (...) { std::cerr << "profile_cleanup_failed\n"; }
        }
        if (sid) FreeSid(sid);
    }
    Container(const Container&) = delete;
    Container& operator=(const Container&) = delete;
};

void grant_execute(const fs::path& path, PSID sid) {
    PACL old_acl = nullptr;
    PSECURITY_DESCRIPTOR descriptor = nullptr;
    const DWORD read_result = GetNamedSecurityInfoW(path.c_str(), SE_FILE_OBJECT,
        DACL_SECURITY_INFORMATION, nullptr, nullptr, &old_acl, nullptr, &descriptor);
    require(read_result == ERROR_SUCCESS, "Read staged ACL");
    EXPLICIT_ACCESSW entry{};
    entry.grfAccessPermissions = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
    entry.grfAccessMode = GRANT_ACCESS;
    entry.Trustee.TrusteeForm = TRUSTEE_IS_SID;
    entry.Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
    entry.Trustee.ptstrName = static_cast<LPWSTR>(sid);
    PACL new_acl = nullptr;
    const DWORD merge_result = SetEntriesInAclW(1, &entry, old_acl, &new_acl);
    DWORD write_result = merge_result;
    if (merge_result == ERROR_SUCCESS) {
        write_result = SetNamedSecurityInfoW(const_cast<LPWSTR>(path.c_str()), SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION, nullptr, nullptr, new_acl, nullptr);
    }
    if (new_acl) LocalFree(new_acl);
    LocalFree(descriptor);
    require(write_result == ERROR_SUCCESS, "Grant staged read/execute");
}

void verify_token(HANDLE process, PSID expected_sid) {
    HANDLE raw_token = nullptr;
    require(OpenProcessToken(process, TOKEN_QUERY, &raw_token) != FALSE, "Open child token");
    Handle token(raw_token);
    const auto data = token_value(token.value, TokenIsAppContainer);
    require(*reinterpret_cast<const DWORD*>(data.data()) == 1, "AppContainer token");
    const auto package = token_value(token.value, TokenAppContainerSid);
    const auto* app = reinterpret_cast<const TOKEN_APPCONTAINER_INFORMATION*>(package.data());
    require(EqualSid(app->TokenAppContainer, expected_sid) != FALSE, "Expected package SID");
    const auto caps = token_value(token.value, TokenCapabilities);
    require(reinterpret_cast<const TOKEN_GROUPS*>(caps.data())->GroupCount == 0, "Zero capabilities");
    const auto integrity = token_value(token.value, TokenIntegrityLevel);
    const auto* label = reinterpret_cast<const TOKEN_MANDATORY_LABEL*>(integrity.data());
    const DWORD count = *GetSidSubAuthorityCount(label->Label.Sid);
    require(*GetSidSubAuthority(label->Label.Sid, count - 1) <= SECURITY_MANDATORY_LOW_RID, "Low integrity");
    PROCESS_MITIGATION_SYSTEM_CALL_DISABLE_POLICY win32k{};
    require(GetProcessMitigationPolicy(process, ProcessSystemCallDisablePolicy, &win32k, sizeof(win32k)) &&
        win32k.DisallowWin32kSystemCalls, "Win32k lockdown");
    PROCESS_MITIGATION_CHILD_PROCESS_POLICY children{};
    require(GetProcessMitigationPolicy(process, ProcessChildProcessPolicy, &children, sizeof(children)) &&
        children.NoChildProcessCreation, "Child creation restriction");
}

struct Attributes {
    std::vector<uintptr_t> storage;
    LPPROC_THREAD_ATTRIBUTE_LIST list = nullptr;
    Attributes() {
        SIZE_T size = 0;
        InitializeProcThreadAttributeList(nullptr, 5, 0, &size);
        storage.resize((size + sizeof(uintptr_t) - 1) / sizeof(uintptr_t));
        list = reinterpret_cast<LPPROC_THREAD_ATTRIBUTE_LIST>(storage.data());
        require(InitializeProcThreadAttributeList(list, 5, 0, &size) != FALSE, "Initialize attributes");
    }
    void add(DWORD_PTR key, void* value, SIZE_T size) {
        require(UpdateProcThreadAttribute(list, 0, key, value, size, nullptr, nullptr) != FALSE,
            "Set process attribute");
    }
    ~Attributes() { if (list) DeleteProcThreadAttributeList(list); }
    Attributes(const Attributes&) = delete;
    Attributes& operator=(const Attributes&) = delete;
};

struct Child {
    Handle process;
    Handle thread;
    explicit Child(const PROCESS_INFORMATION& info) : process(info.hProcess), thread(info.hThread) {}
    ~Child() {
        if (WaitForSingleObject(process.value, 0) == WAIT_TIMEOUT) {
            TerminateProcess(process.value, 99);
            WaitForSingleObject(process.value, 5000);
        }
    }
    DWORD wait(DWORD timeout = 10000) {
        require(WaitForSingleObject(process.value, timeout) == WAIT_OBJECT_0, "Bounded child exit");
        DWORD code = 0;
        require(GetExitCodeProcess(process.value, &code) != FALSE, "Child exit code");
        return code;
    }
};

PROCESS_INFORMATION launch(const fs::path& exe, const std::wstring& args, const fs::path& run,
    PSID sid, HANDLE job, bool less_privileged) {
    SECURITY_CAPABILITIES capabilities{};
    capabilities.AppContainerSid = sid;
    DWORD lpac = less_privileged ? PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT : 0;
    DWORD child_policy = PROCESS_CREATION_CHILD_PROCESS_RESTRICTED;
    DWORD64 mitigations = PROCESS_CREATION_MITIGATION_POLICY_WIN32K_SYSTEM_CALL_DISABLE_ALWAYS_ON;
    Attributes attrs;
    attrs.add(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, &capabilities, sizeof(capabilities));
    attrs.add(PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY, &lpac, sizeof(lpac));
    attrs.add(PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY, &child_policy, sizeof(child_policy));
    attrs.add(PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY, &mitigations, sizeof(mitigations));
    attrs.add(PROC_THREAD_ATTRIBUTE_JOB_LIST, &job, sizeof(job));
    STARTUPINFOEXW startup{};
    startup.StartupInfo.cb = sizeof(startup);
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.lpAttributeList = attrs.list;
    std::wstring command = L"\"" + exe.wstring() + L"\" " + args;
    wchar_t windows[MAX_PATH]{};
    require(GetWindowsDirectoryW(windows, MAX_PATH) > 0, "Windows directory");
    // A minimal environment avoids passing credentials or user configuration.
    wchar_t local_app_data[32768]{};
    require(GetEnvironmentVariableW(L"LOCALAPPDATA", local_app_data, 32768) > 0,
        "AppContainer profile lookup environment");
    std::wstring environment = L"LOCALAPPDATA=" + std::wstring(local_app_data);
    environment.push_back(0);
    environment += L"SystemRoot=" + std::wstring(windows);
    environment.push_back(0);
    environment.push_back(0);
    PROCESS_INFORMATION process{};
    const BOOL created = CreateProcessW(exe.c_str(), command.data(), nullptr, nullptr, FALSE,
        EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
        environment.data(), run.c_str(), &startup.StartupInfo, &process);
    require(created != FALSE, "Create suspended LPAC child");
    return process;
}

int wmain(int argc, wchar_t** argv) {
    try {
        require(argc == 2 && (std::wstring(argv[1]) == L"--appcontainer" ||
            std::wstring(argv[1]) == L"--lpac" || std::wstring(argv[1]) == L"--crash" ||
            std::wstring(argv[1]) == L"--cleanup"), "Select a fixed probe mode");
        const bool less_privileged = std::wstring(argv[1]) == L"--lpac";
        const fs::path run = fs::current_path();
        require(fs::is_regular_file(run / L".rust-browser-isolation-probe"), "Fresh owned run marker");
        const fs::path exe = run / L"boundary-child.exe";
        require(fs::is_regular_file(exe), "Staged probe executable");
        const auto identifier = run.filename().wstring();
        require(identifier.size() == 32 && identifier.find_first_not_of(L"0123456789abcdef") == std::wstring::npos,
            "Run identifier must be a generated GUID");
        const std::wstring name = L"RustBrowser.Probe." + run.filename().wstring();
        if (std::wstring(argv[1]) == L"--cleanup") {
            require(fs::is_regular_file(run / L"profile-created.marker"), "Owned profile creation journal");
            require(SUCCEEDED(DeleteAppContainerProfile(name.c_str())), "Recover owned profile");
            require(DeleteFileW((run / L"profile-created.marker").c_str()) != FALSE, "Remove recovery journal");
            std::cout << "owned_profile_recovery=passed\n";
            return 0;
        }
        Container container(name);
        container.record_creation(run);
        grant_execute(run, container.sid);
        grant_execute(exe, container.sid);
        const fs::path sentinel = run / L"private-sentinel.txt";
        Handle sentinel_file(CreateFileW(sentinel.c_str(), GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr));
        require(sentinel_file.value != INVALID_HANDLE_VALUE, "Reserve private synthetic file");
        // Explicit user-only DACL prevents inherited ALL APPLICATION PACKAGES grants.
        HANDLE raw = nullptr;
        require(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw) != FALSE, "Parent token");
        Handle parent_token(raw);
        const auto user = token_value(parent_token.value, TokenUser);
        EXPLICIT_ACCESSW private_access{};
        private_access.grfAccessPermissions = GENERIC_ALL;
        private_access.grfAccessMode = SET_ACCESS;
        private_access.Trustee.TrusteeForm = TRUSTEE_IS_SID;
        private_access.Trustee.ptstrName = static_cast<LPWSTR>(
            reinterpret_cast<const TOKEN_USER*>(user.data())->User.Sid);
        PACL private_acl = nullptr;
        require(SetEntriesInAclW(1, &private_access, nullptr, &private_acl) == ERROR_SUCCESS, "Private ACL");
        const DWORD secured = SetNamedSecurityInfoW(const_cast<LPWSTR>(sentinel.c_str()), SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, nullptr, nullptr, private_acl, nullptr);
        LocalFree(private_acl);
        require(secured == ERROR_SUCCESS, "Protect synthetic sentinel");
        require(CloseHandle(sentinel_file.value) != FALSE, "Release synthetic sentinel");
        sentinel_file.value = nullptr;
        {
            Handle control_file(CreateFileW(sentinel.c_str(), GENERIC_READ | GENERIC_WRITE,
                0, nullptr, OPEN_EXISTING, 0, nullptr));
            require(control_file.value != INVALID_HANDLE_VALUE, "Parent file positive control");
        }
        const fs::path apps_sentinel = run / L"apps-readable.txt";
        {
            Handle file(CreateFileW(apps_sentinel.c_str(), GENERIC_READ | GENERIC_WRITE, 0,
                nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr));
            require(file.value != INVALID_HANDLE_VALUE, "Reserve apps-readable sentinel");
        }
        BYTE apps_sid[SECURITY_MAX_SID_SIZE]{};
        DWORD apps_sid_size = sizeof(apps_sid);
        require(CreateWellKnownSid(WinBuiltinAnyPackageSid, nullptr, apps_sid, &apps_sid_size) != FALSE,
            "ALL APPLICATION PACKAGES SID");
        grant_execute(apps_sentinel, apps_sid);
        SECURITY_ATTRIBUTES event_attributes{sizeof(SECURITY_ATTRIBUTES), nullptr, TRUE};
        Handle event(CreateEventW(&event_attributes, TRUE, FALSE, nullptr));
        require(event.value != nullptr, "Parent sentinel event");
        WSADATA wsa{};
        require(WSAStartup(MAKEWORD(2, 2), &wsa) == 0, "Parent Winsock initialization");
        SOCKET listener = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        require(listener != INVALID_SOCKET, "Loopback listener");
        sockaddr_in address{};
        address.sin_family = AF_INET;
        address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        require(bind(listener, reinterpret_cast<sockaddr*>(&address), sizeof(address)) == 0, "Bind loopback");
        require(listen(listener, 2) == 0, "Listen loopback");
        int length = sizeof(address);
        require(getsockname(listener, reinterpret_cast<sockaddr*>(&address), &length) == 0, "Loopback port");
        SOCKET control = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        require(control != INVALID_SOCKET, "Positive network control");
        require(connect(control, reinterpret_cast<sockaddr*>(&address), sizeof(address)) == 0,
            "Parent reaches listening endpoint");
        closesocket(control);
        Handle job(CreateJobObjectW(nullptr, nullptr));
        require(job.value != nullptr, "Create lifetime job");
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        require(SetInformationJobObject(job.value, JobObjectExtendedLimitInformation, &limits, sizeof(limits)) != FALSE,
            "Set job limits");
        const std::wstring args = std::wstring(argv[1]) + L" \"" + sentinel.wstring() + L"\" " +
            std::to_wstring(GetCurrentProcessId()) + L" " +
            std::to_wstring(reinterpret_cast<uintptr_t>(event.value)) + L" " +
            std::to_wstring(ntohs(address.sin_port)) + L" \"" + apps_sentinel.wstring() + L"\"";
        if (std::wstring(argv[1]) == L"--crash") {
            const auto info = launch(exe, L"--hold", run, container.sid, job.value, false);
            Child child(info);
            verify_token(child.process.value, container.sid);
            require(ResumeThread(child.thread.value) != static_cast<DWORD>(-1), "Resume crash case");
            FILETIME created{}, exited{}, kernel{}, user_time{};
            require(GetProcessTimes(child.process.value, &created, &exited, &kernel, &user_time), "Child creation time");
            ULARGE_INTEGER stamp{};
            stamp.LowPart = created.dwLowDateTime;
            stamp.HighPart = created.dwHighDateTime;
            const std::string identity = std::to_string(info.dwProcessId) + " " + std::to_string(stamp.QuadPart);
            Handle record(CreateFileW((run / L"child-identity.txt").c_str(), GENERIC_WRITE, 0,
                nullptr, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr));
            require(record.value != INVALID_HANDLE_VALUE, "Reserve child identity");
            DWORD written = 0;
            require(WriteFile(record.value, identity.data(), static_cast<DWORD>(identity.size()), &written, nullptr) &&
                written == identity.size(), "Write child identity");
            require(CloseHandle(record.value), "Close child identity");
            record.value = nullptr;
            TerminateProcess(GetCurrentProcess(), 74); // Deliberately bypass destructors to test recovery.
            return 99;
        }
        {
            Child child(launch(exe, args, run, container.sid, job.value, less_privileged));
            verify_token(child.process.value, container.sid);
            require(ResumeThread(child.thread.value) != static_cast<DWORD>(-1), "Resume probe");
            const DWORD code = child.wait();
            std::cout << "probe_exit_code=" << code << "\n";
            if (code != 0 && code != 2) {
                throw std::runtime_error("Child boundary assertions failed with code=" + std::to_string(code));
            }
            std::cout << "network_result=" << (code == 0 ? "WSAEACCES" : "timeout_unconfirmed") << "\n";
            require(WaitForSingleObject(event.value, 0) == WAIT_TIMEOUT, "Parent event was not inherited");
        }
        closesocket(listener);
        WSACleanup();
        std::cout << "token_file_process_handles=passed\n";
        // Startup must fail without resuming or running a fallback executable.
        bool startup_failed = false;
        try {
            Child unexpected(launch(run / L"missing.exe", L"", run, container.sid, job.value, less_privileged));
        } catch (const WinFailure& failure) { startup_failed = failure.code == ERROR_FILE_NOT_FOUND; }
        require(startup_failed, "Missing executable refused");
        {
            Child child(launch(exe, L"--hold", run, container.sid, job.value, less_privileged));
            verify_token(child.process.value, container.sid);
            require(ResumeThread(child.thread.value) != static_cast<DWORD>(-1), "Resume owned hold child");
            require(TerminateJobObject(job.value, 73) != FALSE, "Terminate owned job");
            require(child.wait() == 73, "Abnormal child exit observed");
        }
        {
            Handle closing_job(CreateJobObjectW(nullptr, nullptr));
            require(closing_job.value != nullptr, "Create close test job");
            require(SetInformationJobObject(closing_job.value, JobObjectExtendedLimitInformation,
                &limits, sizeof(limits)) != FALSE, "Close test job limits");
            Child child(launch(exe, L"--hold", run, container.sid, closing_job.value, less_privileged));
            require(ResumeThread(child.thread.value) != static_cast<DWORD>(-1), "Resume close test");
            require(CloseHandle(closing_job.value) != FALSE, "Close last job handle");
            closing_job.value = nullptr;
            child.wait();
        }
        require(container.remove(), "Delete owned AppContainer profile");
        std::cout << "failed_start_abnormal_exit_job_close_profile_cleanup=passed\n";
        return 0;
    } catch (const std::exception& error) {
        std::cerr << error.what() << "\n";
        return 1;
    }
}
