; KimiCodeBar NSIS 安装/卸载扩展。
;
; 安全边界：
; - 仅清理 KimiCodeBar 自身在 %APPDATA% 下的配置与 Windows 凭据；
; - 绝不删除用户的 ~/.kimi-code、KIMI_CODE_HOME 或 sessions，
;   这些目录属于 Kimi CLI，并可能被其他工具共享。

!macro KIMICODEBAR_DELETE_CREDENTIAL TARGET
  ; 凭据不存在时 cmdkey 会返回非零状态；卸载仍应继续，因此只消费返回码。
  nsExec::Exec '"$SYSDIR\cmdkey.exe" /delete:${TARGET}'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; 静默卸载没有复选框，可显式传入 /DELETEAPPDATA 请求完整清理。
  ${GetOptions} $CMDLINE "/DELETEAPPDATA" $R0
  ${IfNot} ${Errors}
    StrCpy $DeleteAppDataCheckboxState 1
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; 更新安装会复用卸载流程，不能在更新期间删除配置、凭据或自启动设置。
  ${If} $UpdateMode <> 1
    ; auto-launch 还会写入任务管理器的启动审批项；主 Run 项由 Tauri 默认卸载器清理。
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"

    ; 仅在用户勾选“删除应用数据”或传入 /DELETEAPPDATA 时执行完整清理。
    ${If} $DeleteAppDataCheckboxState = 1
      SetShellVarContext current

      ; 应用自己的 settings.json、credentials.json、scan-state.json 位于此目录。
      RmDir /r "$APPDATA\KimiCodeBar"

      ; keyring Windows 后端的目标名规则为 username.service。
      !insertmacro KIMICODEBAR_DELETE_CREDENTIAL "api_key.KimiCodeBar"
      !insertmacro KIMICODEBAR_DELETE_CREDENTIAL "web_token.KimiCodeBar"
      !insertmacro KIMICODEBAR_DELETE_CREDENTIAL "opencode_go_credentials.KimiCodeBar"
    ${EndIf}
  ${EndIf}
!macroend
