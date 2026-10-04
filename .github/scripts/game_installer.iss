; Stable per-game identity: newer builds upgrade the same install and shortcuts.
[Setup]
AppId=BlueEngineGames.{#AppSlug}
AppName={#AppName}
AppVersion={#ReleaseTag}
AppPublisher=BlueEngine Games
DefaultDirName={code:GameDirectory}
DefaultGroupName=BlueEngine Games
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UsePreviousAppDir=yes
DisableProgramGroupPage=yes
DisableDirPage=no
OutputDir={#OutputDir}
OutputBaseFilename={#AppSlug}-setup-windows-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\{#AppExe}
CloseApplications=yes
RestartApplications=no
#ifdef AppIcon
SetupIconFile={#AppIcon}
#endif
[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
; No broad cleanup: player saves and settings survive upgrades and uninstallation.
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\BlueEngine Games\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"
Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Tasks: desktopicon

Name: "{userprograms}\BlueEngine Games\{#AppName} - Check for updates"; Filename: "{app}\Update-{#AppSlug}.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[Code]
function GameDirectory(Param: String): String;
var
  Legacy: String;
begin
  Legacy := ExpandConstant('{localappdata}\BlueEngineLauncher\games\{#AppSlug}');
  if DirExists(Legacy) then
    Result := Legacy
  else
    Result := ExpandConstant('{localappdata}\{#AppFolder}');
end;
