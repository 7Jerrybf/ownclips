; Hooks into Tauri's NSIS installer (tauri.conf.json > bundle > windows >
; nsis > installerHooks).

; Up to 0.5.0 OwnClips was installed per user (%LOCALAPPDATA%\OwnClips); it
; now goes to Program Files for all users. Tauri only removes a previous
; install of the same kind, so uninstall the per-user copy here (silently,
; in place, like Tauri does). Recordings and settings live elsewhere and
; are kept.
!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $R8 HKCU "${UNINSTKEY}" "UninstallString"
  ReadRegStr $R9 HKCU "${MANUPRODUCTKEY}" ""
  ${If} $R8 != ""
  ${AndIf} $R9 != ""
  ${AndIf} $R9 != $INSTDIR
    ExecWait '$R8 /S _?=$R9'
    ; Run in place, the uninstaller can't delete itself or its folder.
    Delete "$R9\uninstall.exe"
    RMDir "$R9"
    ; The uninstaller removed its keys from HKCU; make sure, so this only runs once.
    DeleteRegKey HKCU "${UNINSTKEY}"
    DeleteRegKey HKCU "${MANUPRODUCTKEY}"
  ${EndIf}
!macroend
