!warning "installer.nsh loaded"
Function .onInit
    StrCpy $INSTDIR "$PROGRAMFILES64\invoice-assistant"
FunctionEnd

!macro customInstall
    ; 删除Tauri默认创建的英文桌面快捷方式
    Delete "$DESKTOP\invoice-assistant.lnk"
    ; 删除开始菜单中的英文快捷方式
    RMDir /r "$SMPROGRAMS\invoice-assistant"
    ; 创建中文名桌面快捷方式
    CreateShortcut "$DESKTOP\发票助手.lnk" "$INSTDIR\invoice-assistant.exe"
    ; 创建中文名开始菜单快捷方式
    CreateDirectory "$SMPROGRAMS\发票助手"
    CreateShortcut "$SMPROGRAMS\发票助手\发票助手.lnk" "$INSTDIR\invoice-assistant.exe"
!macroend

!macro customUnInstall
    Delete "$DESKTOP\发票助手.lnk"
    Delete "$DESKTOP\invoice-assistant.lnk"
    RMDir /r "$SMPROGRAMS\发票助手"
    RMDir /r "$SMPROGRAMS\invoice-assistant"
!macroend
