/* PathCchStripPrefix для Windows 7 (в системе она появилась в Windows 8): убирает «\\?\»
   и «\\?\UNC\» в начале пути. Сборка — tools/build-win7.sh. */
#include <windows.h>
#include <string.h>

__declspec(dllexport) HRESULT WINAPI PathCchStripPrefix(PWSTR path, size_t cch) {
    if (!path || cch == 0) return E_INVALIDARG;
    if (cch >= 8 && wcsncmp(path, L"\\\\?\\UNC\\", 8) == 0) {
        size_t n = wcslen(path + 8);
        memmove(path + 2, path + 8, (n + 1) * sizeof(WCHAR));
        return S_OK;
    }
    if (cch >= 4 && wcsncmp(path, L"\\\\?\\", 4) == 0 && path[5] == L':') {
        size_t n = wcslen(path + 4);
        memmove(path, path + 4, (n + 1) * sizeof(WCHAR));
        return S_OK;
    }
    return S_FALSE;
}
