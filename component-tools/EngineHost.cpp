// x86 host for the pinned Jiaoyuelian 1.06 C ABI.
// Confirmed against natpierce.exe's original call at 0x0044f370:
// push 33272; call Run thunk; pop ecx. Run is int __cdecl Run(int port).
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <wincrypt.h>
#include <string>
#include <cstdio>
#include <filesystem>
#include <optional>
#include <cstdint>

#pragma comment(lib, "advapi32.lib")

namespace {
SRWLOCK diagnosticLock = SRWLOCK_INIT;
wchar_t diagnosticFile[32768] = {};
wchar_t diagnosticTemporaryFile[32768] = {};
DWORD diagnosticProcessId = 0;
bool diagnosticElevated = false;
bool diagnosticHasError = false;
bool diagnosticHasPrimaryError = false;

bool process_is_elevated() {
    HANDLE token = nullptr;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) return false;
    TOKEN_ELEVATION elevation = {};
    DWORD size = 0;
    const bool available = GetTokenInformation(token, TokenElevation, &elevation, sizeof(elevation), &size) != FALSE;
    CloseHandle(token);
    return available && elevation.TokenIsElevated != 0;
}

void initialize_diagnostics(const std::wstring& dataDirectory) {
    diagnosticProcessId = GetCurrentProcessId();
    diagnosticElevated = process_is_elevated();
    diagnosticHasError = false;
    diagnosticHasPrimaryError = false;
    const auto file = (std::filesystem::path(dataDirectory) / L"wintun-error.json").wstring();
    const auto temporary = file + L".tmp-" + std::to_wstring(diagnosticProcessId);
    if (file.size() >= _countof(diagnosticFile) || temporary.size() >= _countof(diagnosticTemporaryFile)) return;
    wcscpy_s(diagnosticFile, file.c_str());
    wcscpy_s(diagnosticTemporaryFile, temporary.c_str());
    DeleteFileW(diagnosticFile);
    DeleteFileW(diagnosticTemporaryFile);
}

std::optional<DWORD> hexadecimal_value(LPCWSTR message, LPCWSTR marker) {
    LPCWSTR value = nullptr;
    const auto markerLength = wcslen(marker);
    for (auto match = wcsstr(message, marker); match; match = wcsstr(match + markerLength, marker)) {
        value = match + markerLength;
    }
    if (!value) return std::nullopt;
    DWORD result = 0;
    bool found = false;
    for (; *value; ++value) {
        unsigned digit;
        if (*value >= L'0' && *value <= L'9') digit = *value - L'0';
        else if (*value >= L'a' && *value <= L'f') digit = *value - L'a' + 10;
        else if (*value >= L'A' && *value <= L'F') digit = *value - L'A' + 10;
        else break;
        if (result > 0x0fffffff) return std::nullopt;
        result = result * 16 + digit;
        found = true;
    }
    return found ? std::optional<DWORD>(result) : std::nullopt;
}

const char* diagnostic_stage(LPCWSTR message) {
    struct StagePrefix { LPCWSTR prefix; const char* stage; };
    static const StagePrefix prefixes[] = {
        { L"Failed to setup adapter", "adapterSetup" },
        { L"Timed out waiting for device query", "interfaceWait" },
        { L"Failed to wait for device query", "interfaceWait" },
        { L"Failed to allocate ring memory", "sessionStart" },
        { L"Failed to register rings", "sessionStart" },
        { L"Failed to open adapter device object", "sessionStart" },
        { L"Failed to create send event", "sessionStart" },
        { L"Failed to create receive event", "sessionStart" },
        { L"Could not install driver", "driverInstall" },
        { L"Failed to extract driver", "driverInstall" },
        { L"Failed to take driver installation mutex", "driverInstall" },
        { L"Failed to unload existing driver", "driverInstall" },
        { L"Failed to remove", "cleanup" },
        { L"Unable to remove", "cleanup" },
        { L"Failed to create", "adapterCreate" },
        { L"Failed to initiate", "adapterCreate" }
    };
    for (const auto& prefix : prefixes) {
        if (wcsncmp(message, prefix.prefix, wcslen(prefix.prefix)) == 0) return prefix.stage;
    }
    return "wintun";
}

std::string json_number(const std::optional<DWORD>& value) {
    return value ? std::to_string(*value) : "null";
}

bool write_diagnostic(DWORD64 timestamp, LPCWSTR message) {
    const auto win32Code = json_number(hexadecimal_value(message, L"(Code 0x"));
    const auto problemCode = json_number(hexadecimal_value(message, L"problem code: 0x"));
    const auto ntStatus = json_number(hexadecimal_value(message, L"ntstatus: 0x"));
    char json[512];
    const auto length = snprintf(json, sizeof(json),
        "{\"stage\":\"%s\",\"processId\":%lu,\"timestamp\":%llu,\"elevated\":%s,\"win32Code\":%s,\"problemCode\":%s,\"ntStatus\":%s}\n",
        diagnostic_stage(message), diagnosticProcessId, static_cast<unsigned long long>(timestamp),
        diagnosticElevated ? "true" : "false", win32Code.c_str(), problemCode.c_str(), ntStatus.c_str());
    if (length <= 0 || static_cast<size_t>(length) >= sizeof(json)) return false;
    const auto file = CreateFileW(diagnosticTemporaryFile, GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return false;
    DWORD written = 0;
    bool complete = WriteFile(file, json, static_cast<DWORD>(length), &written, nullptr) != FALSE && written == static_cast<DWORD>(length);
    if (complete) complete = FlushFileBuffers(file) != FALSE;
    CloseHandle(file);
    if (complete) complete = MoveFileExW(diagnosticTemporaryFile, diagnosticFile, MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) != FALSE;
    if (!complete) DeleteFileW(diagnosticTemporaryFile);
    return complete;
}

void WINAPI wintun_logger(DWORD level, DWORD64 timestamp, LPCWSTR message) noexcept {
    const auto previousError = GetLastError();
    if (message && diagnosticFile[0]) {
        AcquireSRWLockExclusive(&diagnosticLock);
        try {
            if (level == 0 && wcscmp(message, L"Creating adapter") == 0) {
                DeleteFileW(diagnosticFile);
                diagnosticHasError = false;
                diagnosticHasPrimaryError = false;
            } else if (level == 2 && !diagnosticHasPrimaryError) {
                const auto stage = diagnostic_stage(message);
                if (!(diagnosticHasError && strcmp(stage, "cleanup") == 0) && write_diagnostic(timestamp, message)) {
                    diagnosticHasError = true;
                    diagnosticHasPrimaryError = strcmp(stage, "adapterSetup") == 0 &&
                        (hexadecimal_value(message, L"problem code: 0x").has_value() ||
                         hexadecimal_value(message, L"ntstatus: 0x").has_value());
                }
            }
        } catch (...) {
            // Diagnostics preserve the native caller's error and execution.
        }
        ReleaseSRWLockExclusive(&diagnosticLock);
    }
    SetLastError(previousError);
}
using WintunLogger = void(WINAPI*)(DWORD, DWORD64, LPCWSTR);
using WintunSetLogger = void(WINAPI*)(WintunLogger);

bool has_pinned_sdk_hash(HMODULE module) {
    static const BYTE expectedHash[] = {
        0xe5, 0x15, 0x79, 0xae, 0x3f, 0xd1, 0xa1, 0xc5,
        0x0d, 0x88, 0x12, 0xbe, 0xf2, 0xee, 0xaa, 0xe1,
        0x97, 0x13, 0x83, 0x7b, 0x4e, 0xcd, 0x72, 0xcc,
        0x31, 0x2f, 0xe0, 0xc1, 0x84, 0x60, 0x9c, 0x96
    };
    wchar_t path[32768];
    const auto pathLength = GetModuleFileNameW(module, path, _countof(path));
    if (!pathLength || pathLength == _countof(path)) return false;
    const auto file = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return false;
    HCRYPTPROV provider = 0;
    HCRYPTHASH hash = 0;
    bool valid = CryptAcquireContextW(&provider, nullptr, nullptr, PROV_RSA_AES, CRYPT_VERIFYCONTEXT) != FALSE;
    if (valid) valid = CryptCreateHash(provider, CALG_SHA_256, 0, 0, &hash) != FALSE;
    BYTE buffer[65536];
    while (valid) {
        DWORD read = 0;
        if (!ReadFile(file, buffer, sizeof(buffer), &read, nullptr)) { valid = false; break; }
        if (!read) break;
        valid = CryptHashData(hash, buffer, read, 0) != FALSE;
    }
    BYTE actualHash[sizeof(expectedHash)];
    DWORD hashSize = sizeof(actualHash);
    if (valid) valid = CryptGetHashParam(hash, HP_HASHVAL, actualHash, &hashSize, 0) != FALSE &&
        hashSize == sizeof(expectedHash) && memcmp(actualHash, expectedHash, sizeof(expectedHash)) == 0;
    if (hash) CryptDestroyHash(hash);
    if (provider) CryptReleaseContext(provider, 0);
    CloseHandle(file);
    return valid;
}

bool sdk_image_region(HMODULE module, const void* address, size_t length, DWORD imageSize, bool writable) {
    const auto base = reinterpret_cast<uintptr_t>(module);
    const auto pointer = reinterpret_cast<uintptr_t>(address);
    if (pointer < base || pointer - base > imageSize || length > imageSize - (pointer - base)) return false;
    MEMORY_BASIC_INFORMATION region = {};
    if (!VirtualQuery(address, &region, sizeof(region)) || region.AllocationBase != module ||
        region.State != MEM_COMMIT || region.Type != MEM_IMAGE ||
        (region.Protect & (PAGE_NOACCESS | PAGE_GUARD))) return false;
    const auto end = reinterpret_cast<uintptr_t>(region.BaseAddress) + region.RegionSize;
    if (length > end - pointer) return false;
    const auto protection = region.Protect & 0xff;
    return !writable || protection == PAGE_READWRITE || protection == PAGE_WRITECOPY;
}

bool bind_sdk_wintun_logging(HMODULE module) {
    // Pinned Jiaoyuelian 1.06, SHA256 e51579ae...84609c96. Its Go lazyDLL.Load
    // invokes the optional OnLoad funcval after resolving network.dll. The
    // verified setupLogger function only installs Go's Wintun logger. Disable
    // that optional data callback before Run so the native logger remains set.
    // This is limited to this SDK image and leaves the on-disk DLL intact.
    if (sizeof(void*) != 4 || !has_pinned_sdk_hash(module)) return false;
    const auto base = reinterpret_cast<BYTE*>(module);
    const auto dos = reinterpret_cast<const IMAGE_DOS_HEADER*>(base);
    if (dos->e_magic != IMAGE_DOS_SIGNATURE || dos->e_lfanew <= 0 || dos->e_lfanew > 4096) return false;
    const auto headers = reinterpret_cast<const IMAGE_NT_HEADERS32*>(base + dos->e_lfanew);
    if (headers->Signature != IMAGE_NT_SIGNATURE || headers->FileHeader.Machine != IMAGE_FILE_MACHINE_I386 ||
        headers->OptionalHeader.Magic != IMAGE_NT_OPTIONAL_HDR32_MAGIC ||
        headers->OptionalHeader.SizeOfImage != 0x85c000) return false;
    const auto size = headers->OptionalHeader.SizeOfImage;
    const auto lazyDll = reinterpret_cast<DWORD*>(base + 0x3ffe20);
    const auto onLoad = reinterpret_cast<void**>(base + 0x3ffe34);
    const auto funcval = base + 0x59dd1c;
    const auto setupLogger = base + 0x31d6f0;
    if (!sdk_image_region(module, lazyDll, 24, size, false) ||
        !sdk_image_region(module, onLoad, sizeof(*onLoad), size, true) ||
        !sdk_image_region(module, funcval, sizeof(DWORD), size, false) ||
        !sdk_image_region(module, setupLogger, 240, size, false)) return false;
    const auto name = reinterpret_cast<const char*>(static_cast<uintptr_t>(lazyDll[0]));
    if (lazyDll[1] != 11 || !sdk_image_region(module, name, 11, size, false) ||
        memcmp(name, "network.dll", 11) != 0 || *onLoad != funcval ||
        *reinterpret_cast<const DWORD*>(funcval) != reinterpret_cast<uintptr_t>(setupLogger)) return false;
    const auto previous = InterlockedExchangePointer(onLoad, nullptr);
    if (previous != funcval) {
        InterlockedCompareExchangePointer(onLoad, previous, nullptr);
        return false;
    }
    return true;
}
}

std::wstring environment(const wchar_t* name) {
    const auto size = GetEnvironmentVariableW(name, nullptr, 0);
    if (!size) return {};
    std::wstring value(size, L'\0');
    value.resize(GetEnvironmentVariableW(name, value.data(), size));
    return value;
}

int wmain(int argc, wchar_t** argv) {
    // The Go DLL parses this process's argv. Only its verified -p option may be
    // passed; host configuration therefore uses private inherited environment.
    auto engineDirectory = environment(L"LLREMOTE_ENGINE_DIRECTORY");
    auto dataDirectory = environment(L"LLREMOTE_DATA_DIRECTORY");
    auto stopEvent = environment(L"LLREMOTE_STOP_EVENT");
    int port = 33272;
    for (int i = 1; i + 1 < argc; i += 2) {
        const std::wstring key = argv[i];
        if (key == L"-p") port = _wtoi(argv[i + 1]);
        else return 10;
    }
    // Shell elevation does not reliably inherit private environment variables.
    // The host has a fixed resource layout and receives only the original -p arg.
    if (engineDirectory.empty()) {
        wchar_t executable[32768];
        const auto length = GetModuleFileNameW(nullptr, executable, 32768);
        if (!length || length == 32768) return 10;
        engineDirectory = (std::filesystem::path(executable).parent_path() / L"engines" / L"Jiaoyuelian").wstring();
    }
    if (dataDirectory.empty()) {
        wchar_t directory[32768];
        const auto length = GetCurrentDirectoryW(32768, directory);
        if (!length || length >= 32768) return 10;
        dataDirectory = directory;
    }
    if (stopEvent.empty()) stopEvent = L"Local\\LLRemoteNatStop-" + std::to_wstring(port);
    if (port < 1024 || port > 65535) return 10;
    if (!SetCurrentDirectoryW(dataDirectory.c_str())) return 3;
    SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS | LOAD_LIBRARY_SEARCH_USER_DIRS);
    AddDllDirectory(engineDirectory.c_str());
    // The pinned Go Wintun loader searches only APPLICATION_DIR and SYSTEM32.
    // Preload its exact basename from the bundled engine directory so that the
    // later short-name load reuses this module, including after UAC elevation.
    const std::wstring networkLibrary = engineDirectory + L"\\network.dll";
    const auto networkModule = LoadLibraryExW(networkLibrary.c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if (!networkModule) { fprintf(stderr, "HOST_NETWORK_LOAD_ERROR %lu\n", GetLastError()); return 6; }
    const auto networkByName = LoadLibraryExW(L"network.dll", nullptr, LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!networkByName || networkByName != networkModule || !GetProcAddress(networkModule, "WintunCreateAdapter")) {
        fprintf(stderr, "HOST_NETWORK_API_ERROR %lu\n", GetLastError()); return 7;
    }
    const auto setWintunLogger = reinterpret_cast<WintunSetLogger>(GetProcAddress(networkModule, "WintunSetLogger"));
    initialize_diagnostics(dataDirectory);
    const std::wstring library = engineDirectory + L"\\natpierce.dll";
    const auto module = LoadLibraryExW(library.c_str(), nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if (!module) { fprintf(stderr, "HOST_LOAD_ERROR %lu\n", GetLastError()); return 4; }
    const auto run = reinterpret_cast<int(__cdecl*)(int)>(GetProcAddress(module, "Run"));
    const auto state = reinterpret_cast<int(__cdecl*)()>(GetProcAddress(module, "State"));
    const auto exit = reinterpret_cast<void(__cdecl*)()>(GetProcAddress(module, "Exit_1"));
    if (!run || !state || !exit) return 5;
    const bool nativeLogging = setWintunLogger && bind_sdk_wintun_logging(module);
    if (nativeLogging) setWintunLogger(wintun_logger);
    else fprintf(stderr, "HOST_WINTUN_LOGGER_UNSUPPORTED\n");
    run(port);
    if (nativeLogging) setWintunLogger(wintun_logger);
    // No original EXE or browser is launched. Keep the Go DLL loaded until process exit.
    if (!stopEvent.empty()) {
        const auto event = OpenEventW(SYNCHRONIZE, FALSE, stopEvent.c_str());
        if (event) { WaitForSingleObject(event, INFINITE); CloseHandle(event); exit(); Sleep(200); return 0; }
    }
    while (true) Sleep(1000);
}
