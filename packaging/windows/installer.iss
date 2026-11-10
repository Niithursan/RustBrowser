; The production entry point accepts only a validated allowlisted portable payload.
; Validation builds change identity and destination, never production profile paths.
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PayloadRoot
  #error PayloadRoot is required
#endif
#ifndef IconFile
  #error IconFile is required
#endif
#ifdef ValidationId
  #define Identity "RustBrowser.Validation." + ValidationId
  #define ProductName "Rust Browser validation " + ValidationId
  #define InstallDirectory ValidationRoot
  #define RunningMutex "Local\RustBrowser.Validation." + ValidationId
#else
  #define Identity "Niithursan.RustBrowser"
  #define ProductName "Rust Browser"
  #define InstallDirectory "{localappdata}\Programs\RustBrowser"
  #define RunningMutex "Local\RustBrowser.ApplicationRunning"
#endif

[Setup]
AppId={#Identity}
AppName={#ProductName}
AppVersion={#AppVersion}
AppPublisher=Niithursan
AppPublisherURL=https://github.com/Niithursan/RustBrowser
VersionInfoVersion={#AppVersion}
VersionInfoProductName=Rust Browser
VersionInfoProductVersion={#AppVersion}
DefaultDirName={#InstallDirectory}
DefaultGroupName={#ProductName}
UsePreviousAppDir=no
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0.17763
SetupIconFile={#IconFile}
UninstallDisplayIcon={app}\rust-browser.exe
UninstallFilesDir={app}\uninstall
UninstallLogMode=append
SignedUninstaller=no
AppMutex={#RunningMutex}
SetupMutex={#RunningMutex}.Setup
CloseApplications=no
RestartApplications=no
RedirectionGuard=yes
AllowNoIcons=yes
OutputBaseFilename=rust-browser-v{#AppVersion}-windows-x64-setup
Compression=lzma2/normal
SolidCompression=yes
WizardStyle=modern
LicenseFile={#PayloadRoot}\LICENSE
InfoBeforeFile=installer-notice.txt

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#PayloadRoot}\rust-browser.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadRoot}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadRoot}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadRoot}\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadRoot}\SECURITY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadRoot}\VALIDATION.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadRoot}\INSTALL_WINDOWS.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadRoot}\package.json"; DestDir: "{app}"; Flags: ignoreversion

[INI]
Filename: "{app}\installation.ini"; Section: "Installation"; Key: "Identity"; String: "{#Identity}"
Filename: "{app}\installation.ini"; Section: "Installation"; Key: "Version"; String: "{#AppVersion}"

[UninstallDelete]
; Exact installer-owned marker only. Never delete the app tree or any profile.
Type: files; Name: "{app}\installation.ini"
; Setup's own temporary files ("is-*.tmp") left by a Setup that was interrupted
; while replacing the executable or writing the uninstaller. Not recursive.
Type: files; Name: "{app}\is-*.tmp"
Type: files; Name: "{app}\uninstall\is-*.tmp"

[Icons]
Name: "{userprograms}\{#ProductName}\Rust Browser"; Filename: "{app}\rust-browser.exe"; WorkingDir: "{app}"; AppUserModelID: "Niithursan.RustBrowser"
Name: "{userdesktop}\{#ProductName}"; Filename: "{app}\rust-browser.exe"; WorkingDir: "{app}"; Tasks: desktopicon; AppUserModelID: "Niithursan.RustBrowser"

[Code]
function GetFileAttributesW(Name: String): Cardinal;
  external 'GetFileAttributesW@kernel32.dll stdcall';
function OpenForWrite(Name: String; Access, Share: Cardinal; Security: THandle;
  Creation, Flags: Cardinal; Template: THandle): THandle;
  external 'CreateFileW@kernel32.dll stdcall';
function CloseFile(Handle: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

function ExecutableIsWritable(Path: String): Boolean;
var
  Handle: THandle;
begin
  { Open an existing file without writing. Mapped executables (including v0.12,
    which has no mutex) and inaccessible/read-only files must fail before copying. }
  Handle := OpenForWrite(Path, $40000000, 7, 0, 3, $80, 0);
  Result := Handle <> THandle(-1);
  if Result then CloseFile(Handle);
end;

function PathIsSafe(Path: String): Boolean;
var
  Parent: String;
  Attributes: Cardinal;
begin
  Result := False;
  Path := RemoveBackslashUnlessRoot(ExpandFileName(Path));
  if CompareText(Path, RemoveBackslashUnlessRoot(ExpandConstant('{#InstallDirectory}'))) <> 0 then
    exit;
  while Length(Path) > 3 do begin
    Attributes := GetFileAttributesW(Path);
    if (Attributes <> $FFFFFFFF) and ((Attributes and $400) <> 0) then exit;
    if FileExists(Path + '\session.json') or FileExists(Path + '\session.lock') or
       FileExists(Path + '\library.sqlite') then exit;
    Parent := ExtractFileDir(Path);
    if Parent = Path then exit;
    Path := Parent;
  end;
  Result := True;
end;

function HasWebViewRuntime: Boolean;
var
  Version: String;
  Key: String;
begin
  Key := 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  Result := (RegQueryStringValue(HKCU, Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
    (RegQueryStringValue(HKLM32, Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0'));
end;

function VersionIsCanonical(Value: String; var Packed: Int64): Boolean;
var
  Parts: TArrayOfString;
  Index, Number: Integer;
begin
  Result := False;
  Parts := StringSplit(Value, ['.'], stAll);
  if GetArrayLength(Parts) <> 3 then exit;
  for Index := 0 to 2 do begin
    Number := StrToIntDef(Parts[Index], -1);
    if (Number < 0) or (Number > 65535) or (IntToStr(Number) <> Parts[Index]) then exit;
  end;
  Result := StrToVersion(Value, Packed);
end;

function TreeIsOrdinary(Directory: String; Depth: Integer): Boolean;
var
  Found: TFindRec;
  Count: Integer;
begin
  Result := False;
  if Depth > 4 then exit;
  Count := 0;
  if FindFirst(Directory + '\*', Found) then begin
    try
      repeat
        if (Found.Name <> '.') and (Found.Name <> '..') then begin
          Count := Count + 1;
          if (Count > 100) or ((Found.Attributes and $400) <> 0) then exit;
          if ((Found.Attributes and FILE_ATTRIBUTE_DIRECTORY) <> 0) and
            not TreeIsOrdinary(Directory + '\' + Found.Name, Depth + 1) then exit;
          if (CompareText(Found.Name, 'session.json') = 0) or
             (CompareText(Found.Name, 'session.lock') = 0) or
             (CompareText(Found.Name, 'library.sqlite') = 0) then exit;
        end;
      until not FindNext(Found);
    finally
      FindClose(Found);
    end;
  end;
  Result := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Directory, Owner, Previous: String;
  Existing, Incoming: Int64;
  Found: TFindRec;
begin
  Result := '';
  Directory := ExpandConstant('{app}');
  if not PathIsSafe(Directory) or not TreeIsOrdinary(Directory, 0) then begin
    Result := 'Unsafe installation directory. Use the fixed ordinary per-user directory, separate from browser profiles.';
    exit;
  end;
  if DirExists(Directory) and FindFirst(Directory + '\*', Found) then begin
    try
      repeat
        if (Found.Name <> '.') and (Found.Name <> '..') then begin
          Owner := GetIniString('Installation', 'Identity', '', Directory + '\installation.ini');
          if Owner <> '{#Identity}' then begin
            Result := 'The installation directory contains files not owned by this installer. Preserve them and choose portable installation or move them yourself.';
            exit;
          end;
        end;
      until not FindNext(Found);
    finally
      FindClose(Found);
    end;
  end;
  if not VersionIsCanonical('{#AppVersion}', Incoming) then begin
    Result := 'Invalid incoming application version.';
    exit;
  end;
  Previous := GetIniString('Installation', 'Version', '', Directory + '\installation.ini');
  if Previous <> '' then begin
    if not VersionIsCanonical(Previous, Existing) then begin
      Result := 'Installed version metadata is invalid. Preserve the application directory for manual recovery.';
      exit;
    end;
    if ComparePackedVersion(Existing, Incoming) > 0 then begin
      Result := 'Downgrade refused. Restore a compatible closed-profile backup before using an older browser.';
      exit;
    end;
  end;
  if FileExists(Directory + '\rust-browser.exe') then begin
    if not ExecutableIsWritable(Directory + '\rust-browser.exe') then begin
      Result := 'The installed executable is currently in use or not writable. Close every browser window and retry. No process was terminated.';
      exit;
    end;
    if not GetPackedVersion(Directory + '\rust-browser.exe', Existing) then begin
      Result := 'Installed executable version cannot be read; preserve it for manual recovery.';
      exit;
    end;
    if ComparePackedVersion(Existing, Incoming) > 0 then begin
      Result := 'Downgrade refused: the installed executable is newer.';
      exit;
    end;
  end;
  if not HasWebViewRuntime then begin
    Result := 'Microsoft Evergreen WebView2 Runtime is required. Install it from Microsoft, then retry. No runtime was downloaded or installed.';
    exit;
  end;
  if not FileExists(ExpandConstant('{sys}\vcruntime140.dll')) or
     not FileExists(ExpandConstant('{sys}\vcruntime140_1.dll')) then
    Result := 'Microsoft x64 Visual C++ v14 Runtime is required. Install the latest supported version from Microsoft, then retry. No runtime was downloaded or installed.';
end;

// Remove files left by an interrupted Setup from one directory, not recursively.
// Left in place they keep the directory non-empty after uninstall, so the next
// installation is refused as unowned.
procedure RemoveStaleSetupFiles(Directory, Pattern: String);
var
  Found: TFindRec;
begin
  if FindFirst(Directory + '\' + Pattern, Found) then begin
    try
      repeat
        if (Found.Attributes and FILE_ATTRIBUTE_DIRECTORY) = 0 then
          DeleteFile(Directory + '\' + Found.Name);
      until not FindNext(Found);
    finally
      FindClose(Found);
    end;
  end;
end;

// After an interrupted installation Setup cannot append to the unfinished
// uninstall log, so it writes a second one (unins001 beside unins000) and
// registers that. Uninstall then removes only the registered one, the leftover
// keeps the directory non-empty, and the next installation is refused as unowned.
// Once installation has succeeded, keep only the uninstaller Apps & Features runs.
procedure RemoveUnregisteredUninstallers(Directory: String);
var
  Registered, Keep, Extension: String;
  Found: TFindRec;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER,
    'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#Identity}_is1',
    'UninstallString', Registered) then exit;
  Keep := ChangeFileExt(ExtractFileName(RemoveQuotes(Registered)), '');
  if (Length(Keep) <> 8) or (CompareText(Copy(Keep, 1, 5), 'unins') <> 0) then exit;
  if FindFirst(Directory + '\unins???.*', Found) then begin
    try
      repeat
        Extension := LowerCase(ExtractFileExt(Found.Name));
        if ((Found.Attributes and FILE_ATTRIBUTE_DIRECTORY) = 0) and
           ((Extension = '.dat') or (Extension = '.exe') or (Extension = '.msg')) and
           (CompareText(ChangeFileExt(Found.Name, ''), Keep) <> 0) then
          DeleteFile(Directory + '\' + Found.Name);
      until not FindNext(Found);
    finally
      FindClose(Found);
    end;
  end;
end;

// Claim the directory before the first file is copied. PrepareToInstall has just
// verified it is empty or already owned by this identity, so this only marks a
// directory Setup already checked. Without it, a Setup that was killed or lost
// power during the copy left payload files but no marker; every later Setup then
// refused the directory as unowned and the uninstaller refused it too. The [INI]
// entries below rewrite the same values once installation completes.
procedure CurStepChanged(CurStep: TSetupStep);
var
  Marker: String;
begin
  if CurStep = ssInstall then begin
    Marker := ExpandConstant('{app}\installation.ini');
    if not ForceDirectories(ExpandConstant('{app}')) or
       not SetIniString('Installation', 'Identity', '{#Identity}', Marker) or
       not SetIniString('Installation', 'Version', '{#AppVersion}', Marker) then
      RaiseException('Could not record installation ownership. Nothing was copied.');
    // Ownership was verified by PrepareToInstall; only this identity's directory.
    // "is-*.tmp" are Setup's own temporary copies of a file being replaced or of
    // the uninstaller being written.
    RemoveStaleSetupFiles(ExpandConstant('{app}'), 'is-*.tmp');
    RemoveStaleSetupFiles(ExpandConstant('{app}\uninstall'), 'is-*.tmp');
  end;
  if CurStep = ssPostInstall then
    RemoveUnregisteredUninstallers(ExpandConstant('{app}\uninstall'));
end;

// The uninstaller removes only folders Setup recorded creating. After an
// interrupted installation, or because the ownership marker is written before
// Inno Setup creates the folder, some were not recorded. Remove them when empty,
// so a later installation does not find an unowned non-empty folder.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) or (CurUninstallStep = usDone) then begin
    RemoveDir(ExpandConstant('{app}\uninstall'));
    RemoveDir(ExpandConstant('{app}'));
  end;
end;

function InitializeUninstall: Boolean;
begin
  Result := PathIsSafe(ExpandConstant('{app}')) and TreeIsOrdinary(ExpandConstant('{app}'), 0) and
    (GetIniString('Installation', 'Identity', '', ExpandConstant('{app}\installation.ini')) = '{#Identity}');
  if not Result then
    SuppressibleMsgBox('Uninstall refused: installation ownership/path could not be verified. Browser profiles were not changed.', mbError, MB_OK, IDOK);
end;
