; Inno Setup script for Sextant Browser.
;
; Build:  iscc installer\sextant.iss   (after a release build)
; Needs:  rust\target\release\sextant-browser.exe + its runtime DLLs.
;
; Per-user by default (no admin): {autopf} resolves to %LOCALAPPDATA%\Programs
; under "lowest" privileges, or Program Files if the user elevates. The Start
; menu shortcut, optional desktop shortcut, and optional run-on-sign-in are all
; per-user (HKCU), and the uninstaller is generated automatically.

#define AppName "Sextant Browser"
#define AppVersion "0.1.0"
#define AppPublisher "Rustcor"
#define AppExe "sextant-browser.exe"
#define RelDir "..\rust\target\release"
#define IcoFile "..\rust\sextant-hull\assets\icons\sextant.ico"

[Setup]
AppId={{B7E5D4A1-3C2F-4E8A-9B1D-6F0A2C5E7D34}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\Sextant Browser
DefaultGroupName=Sextant Browser
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
OutputDir=..\dist
OutputBaseFilename=sextant-browser-setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
SetupIconFile={#IcoFile}

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"
Name: "autostart"; Description: "Start Sextant Browser when I &sign in"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
Source: "{#RelDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
; Runtime DLLs that ship next to the exe (e.g. html2md.dll from distill).
Source: "{#RelDir}\*.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#IcoFile}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Sextant Browser"; Filename: "{app}\{#AppExe}"; IconFilename: "{app}\sextant.ico"
Name: "{group}\Uninstall Sextant Browser"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Sextant Browser"; Filename: "{app}\{#AppExe}"; IconFilename: "{app}\sextant.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "SextantBrowser"; ValueData: """{app}\{#AppExe}"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch Sextant Browser now"; Flags: nowait postinstall skipifsilent
