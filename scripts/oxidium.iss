; Oxidium Windows Installer (Inno Setup 6)
; Per-user HKCU installation — requires NO administrator elevation.

#define MyAppName "Oxidium"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Oxidium Contributors"
#define MyAppURL "https://github.com/cryals/Oxidium"
#define MyAppExeName "student-journal.exe"
#define MyMcpExeName "student-journal-mcp.exe"

[Setup]
AppId={{8E5F39A2-6D14-4C9B-B82A-7F9A3D61E501}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
LicenseFile=..\LICENSE.txt
OutputDir=..\dist
OutputBaseFilename=Oxidium-{#MyAppVersion}-windows-x86_64-inno-setup
SetupIconFile=..\assets\icon\oxidium.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\{#MyMcpExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\mcp\QUICKSTART.md"; DestDir: "{app}"; DestName: "MCP-QUICKSTART.md"; Flags: ignoreversion
Source: "..\CHANGELOG.md"; DestDir: "{app}"; DestName: "RELEASE-NOTES.md"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent
