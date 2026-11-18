; Installateur Windows de StudentCode, version Rust (Inno Setup 6).
; iscc /DAppVersion=2.0.0 /DSrc=..\target\release packaging\studentcode.iss
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Src
  #define Src "..\target\release"
#endif

[Setup]
AppId={{9E2F4C61-7B3A-4D58-A1C2-3F6B8E0D5A27}
AppName=StudentCode
AppVersion={#AppVersion}
AppPublisher=StudentCode
AppPublisherURL=https://github.com/AntoninMichelq/studentCode
DefaultDirName={localappdata}\Programs\StudentCode-rs
DefaultGroupName=StudentCode
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\out
OutputBaseFilename=StudentCode-{#AppVersion}-windows-x64-setup
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\studentcode.exe
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
CloseApplications=yes
ChangesEnvironment=yes

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

; Choix des modules à l'installation
[Types]
Name: "full"; Description: "Complète (éditeur + PyScripter + lecteur PDF)"
Name: "compact"; Description: "Minimale (éditeur seul, le plus léger)"
Name: "custom"; Description: "Personnalisée"; Flags: iscustom

[Components]
Name: "core"; Description: "Éditeur, explorateur, terminal, exécution C / C++ / Java / Python"; Types: full compact custom; Flags: fixed
Name: "pyscripter"; Description: "Module PyScripter : débogueur Python pas à pas, variables, console Python après exécution, bibliothèques Python"; Types: full
Name: "pdf"; Description: "Module Lecteur PDF : sujets et cours PDF dans un onglet, à côté du code"; Types: full

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "addtopath"; Description: "Commande « studentcode » dans les terminaux (ex. studentcode . pour ouvrir le dossier courant)"; GroupDescription: "Ligne de commande :"

[Files]
Source: "{#Src}\studentcode.exe"; DestDir: "{app}"; Components: core; Flags: ignoreversion
Source: "..\modules\pyscripter\*.py"; DestDir: "{app}\modules\pyscripter"; Components: pyscripter; Flags: ignoreversion
Source: "..\modules\pdf\pdfium.dll"; DestDir: "{app}\modules\pdf"; Components: pdf; Flags: ignoreversion

[InstallDelete]
; si on réinstalle sans le module, l'ancien module est retiré
Type: filesandordirs; Name: "{app}\modules\pyscripter"
Type: filesandordirs; Name: "{app}\modules\pdf"

[Icons]
Name: "{group}\StudentCode"; Filename: "{app}\studentcode.exe"
Name: "{group}\Désinstaller StudentCode"; Filename: "{uninstallexe}"
Name: "{autodesktop}\StudentCode"; Filename: "{app}\studentcode.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\studentcode.exe"; Description: "{cm:LaunchProgram,StudentCode}"; Flags: nowait postinstall skipifsilent

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{app};{olddata}"; \
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
  P := Pos(Uppercase(Dir) + ';', Uppercase(Paths));
  if P > 0 then
  begin
    Delete(Paths, P, Length(Dir) + 1);
    RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Paths);
  end;
end;
