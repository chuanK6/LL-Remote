!macro NSIS_HOOK_PREINSTALL
  ; Component roles are chosen inside LL. Never register independent startup
  ; entries or replace external Sunshine services during installation.
!macroend

!macro NSIS_HOOK_POSTINSTALL
  SetOutPath "$INSTDIR"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Uninstall only the LL startup entry. User data and imported installations
  ; belong to the user and are deliberately retained.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "LLRemote"
!macroend
