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

; The updater can add files after the installer wrote its uninstall log. Remove
; unchanged files from the current package receipt as well, keeping player files.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Lines: TArrayOfString;
  I, Separator: Integer;
  Root, RelativePath, Target, ExpectedHash: String;
begin
  if CurUninstallStep <> usUninstall then Exit;
  Root := AddBackslash(ExpandConstant('{app}'));
  if not LoadStringsFromFile(Root + '.installed-files.tsv', Lines) then Exit;
  for I := 0 to GetArrayLength(Lines) - 1 do begin
    Separator := Pos(#9, Lines[I]);
    if Separator > 1 then begin
      RelativePath := Copy(Lines[I], 1, Separator - 1);
      ExpectedHash := Trim(Copy(Lines[I], Separator + 1, MaxInt));
      Target := ExpandFileName(Root + RelativePath);
      if (CompareText(Copy(Target, 1, Length(Root)), Root) = 0) and
         (Length(ExpectedHash) = 64) and FileExists(Target) then begin
        try
          if CompareText(GetSHA256OfFile(Target), ExpectedHash) = 0 then
            DeleteFile(Target);
        except
          Log('Could not remove package file: ' + Target);
        end;
      end;
    end;
  end;
end;
