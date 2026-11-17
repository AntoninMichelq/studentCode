; Installateur Windows de StudentCode (Inno Setup 6).
; Construit par la fabrication des versions : iscc /DAppVersion=1.2.3 packaging\studentcode.iss
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6C1B8E7A-3F52-4C0E-9B7D-5A1E2D4C8F11}
AppName=StudentCode
AppVersion={#AppVersion}
AppPublisher=StudentCode
AppPublisherURL=https://github.com/AntoninMichelq/studentCode
DefaultDirName={localappdata}\Programs\StudentCode
DefaultGroupName=StudentCode
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

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\dist\StudentCode\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\StudentCode"; Filename: "{app}\StudentCode.exe"
Name: "{group}\Désinstaller StudentCode"; Filename: "{uninstallexe}"
Name: "{autodesktop}\StudentCode"; Filename: "{app}\StudentCode.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\StudentCode.exe"; Description: "{cm:LaunchProgram,StudentCode}"; Flags: nowait postinstall skipifsilent
