; Installateur Windows de StudentCode (Inno Setup 6).
; Construit par la fabrication des versions : iscc /DAppVersion=1.2.3 packaging\studentcode.iss
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6C1B8E7A-3F52-4C0E-9B7D-5A1E2D4C8F11}
AppName=SCode (Python)
AppVersion={#AppVersion}
AppPublisher=SCode
AppPublisherURL=https://github.com/AntoninMichelq/studentCode
DefaultDirName={localappdata}\Programs\StudentCode
DefaultGroupName=SCode (Python)
DisableProgramGroupPage=yes
; installation pour l'utilisateur courant : pas besoin d'être administrateur
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
OutputBaseFilename=StudentCode-{#AppVersion}-windows-x64-setup
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\StudentCode.exe
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
CloseApplications=yes
; le dossier de l'app est ajouté au PATH : « studentcode . » marche dans un terminal
ChangesEnvironment=yes

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "addtopath"; Description: "Commande « studentcode » dans les terminaux (ex. studentcode . pour ouvrir le dossier courant)"; GroupDescription: "Ligne de commande :"

[Files]
Source: "..\dist\StudentCode\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\SCode (Python)"; Filename: "{app}\StudentCode.exe"
Name: "{group}\Désinstaller SCode (Python)"; Filename: "{uninstallexe}"
Name: "{autodesktop}\SCode (Python)"; Filename: "{app}\StudentCode.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\StudentCode.exe"; Description: "{cm:LaunchProgram,SCode}"; Flags: nowait postinstall skipifsilent

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; \
    Tasks: addtopath; Check: NeedsAddPath(ExpandConstant('{app}'))

[Code]
function NeedsAddPath(Dir: string): Boolean;
var
  Paths: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Paths) then
  begin
    Result := True;
    exit;
  end;
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Paths) + ';') = 0;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Paths, Dir: string;
  P: Integer;
begin
  if CurUninstallStep <> usPostUninstall then
    exit;
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Paths) then
    exit;
  Dir := ExpandConstant('{app}');
  P := Pos(';' + Uppercase(Dir), Uppercase(Paths));
  if P > 0 then
  begin
    Delete(Paths, P, Length(Dir) + 1);
    RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Paths);
  end;
end;
