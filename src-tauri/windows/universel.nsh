; Installeur Windows UNIQUE : l'installeur x64 embarque aussi le launcher
; ARM64, et le pose à la place du x64 sur un PC ARM (Snapdragon). Le x64
; tournerait sinon en émulation.
;
; Utilisé par la CI seulement (tauri.universel.conf.json) : elle construit
; d'abord le launcher ARM64 et donne son chemin dans TURI_ARM64_EXE.
; Une mise à jour passe aussi par cet installeur : un launcher x64 installé
; sur un PC ARM redevient natif à la mise à jour suivante.

!macro NSIS_HOOK_POSTINSTALL
  ${If} ${IsNativeARM64}
    File "/oname=$INSTDIR\${MAINBINARYNAME}.exe" "$%TURI_ARM64_EXE%"
  ${EndIf}
!macroend
