; Diwan installer hooks
;
; Runs a friendly minimum-OS check before the installer touches anything
; (before file copy, before the WebView2 offline-installer step). Without
; this, an unsupported machine (e.g. genuine Windows 7) gets partway into
; setup and then fails deep inside WebView2 installation with a cryptic
; native error:
;
;   "MicrosoftEdgeUpdate.exe - Entry Point Not Found. The procedure entry
;    point PackageIdFromFullName could not be located in the dynamic link
;    library KERNEL32.dll."
;
; WebView2 Runtime has no Windows 7/8/8.1 build at all (Microsoft's stated
; minimum is Windows 10), so there is no code fix for those OSes — the best
; we can do is fail early with plain language instead of that error.
;
; WinVer.nsh ships with NSIS itself (not Tauri-specific) and provides the
; ${AtLeastWin10} version-check macro used below; LogicLib (${If}/${EndIf})
; is already pulled in by MUI2.nsh in Tauri's base installer.nsi.
!include "WinVer.nsh"

!macro NSIS_HOOK_PREINSTALL
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_ICONSTOP|MB_OK \
      "This app requires Windows 10 or later.$\r$\n$\r$\nYour system is running an older version of Windows that isn't supported. Please install this app on a Windows 10 (or newer) computer."
    Abort
  ${EndIf}
!macroend
