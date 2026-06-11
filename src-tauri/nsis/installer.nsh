!warning "installer.nsh loaded"
Function .onInit
    StrCpy $INSTDIR "$PROGRAMFILES64\invoice-assistant"
FunctionEnd
