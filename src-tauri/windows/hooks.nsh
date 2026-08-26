; Diwan installer hooks
;
; Runs a friendly minimum-OS check before the installer touches anything
; (before file copy, before any WebView2 install step). Without this, an
; unsupported machine gets partway into setup and then fails deep inside
; WebView2 with a cryptic native error, e.g.:
;
;   "MicrosoftEdgeUpdate.exe - Entry Point Not Found. The procedure entry
;    point PackageIdFromFullName could not be located in the dynamic link
;    library KERNEL32.dll."
;
; Floor is Windows 7 SP1, not because that's comfortable but because it's
; the actual floor: WebView2 has never run on anything older (XP/Vista),
; and Windows 7 support only works at all because bundle.windows.
; webviewInstallMode is pinned to "fixedRuntime" with WebView2 Runtime
; 109.0.1518.140 — the last build Microsoft ever shipped for Win7/8/8.1
; (every WebView2 release since Jan 2023 refuses to start there). See
; DEPLOYMENT.md's "Windows 7 support" section for what that implies
; (frozen, unpatched-since-2023 engine — accepted tradeoff, not an oversight).
;
; WinVer.nsh ships with NSIS itself (not Tauri-specific) and provides the
; ${AtLeastWin7} version-check macro used below; LogicLib (${If}/${EndIf})
; is already pulled in by MUI2.nsh in Tauri's base installer.nsi.
!include "WinVer.nsh"

!macro NSIS_HOOK_PREINSTALL
  ${IfNot} ${AtLeastWin7}
    MessageBox MB_ICONSTOP|MB_OK \
      "This app requires Windows 7 SP1 or later.$\r$\n$\r$\nYour system is running an older version of Windows that isn't supported. Please install this app on a Windows 7 (or newer) computer."
    Abort
  ${EndIf}
!macroend
