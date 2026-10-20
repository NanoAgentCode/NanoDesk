; Remove only NanoDesk's document verbs when uninstalling this installation.
!macro RemoveDocumentVerb EXT
  ReadRegStr $R0 HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\NanoDesk.DocumentChat\command" ""
  StrCmp $R0 '$\"$INSTDIR\nano-desk.exe$\" --document-chat $\"%1$\"' 0 +2
    DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\NanoDesk.DocumentChat"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro RemoveDocumentVerb "txt"
  !insertmacro RemoveDocumentVerb "md"
  !insertmacro RemoveDocumentVerb "markdown"
  !insertmacro RemoveDocumentVerb "docx"
  !insertmacro RemoveDocumentVerb "pdf"
  !insertmacro RemoveDocumentVerb "rtf"
  !insertmacro RemoveDocumentVerb "csv"
  !insertmacro RemoveDocumentVerb "xlsx"
  !insertmacro RemoveDocumentVerb "pptx"
!macroend
