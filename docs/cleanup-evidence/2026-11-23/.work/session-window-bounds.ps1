param([int]$AppId, [int]$MoveBy = 0)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class FixtureWindows {
  [StructLayout(LayoutKind.Sequential)]
  public struct Rect { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)]
  public struct Point { public int X, Y; }
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)]
  public static extern IntPtr FindWindow(string className, string title);
  public delegate bool EnumProc(IntPtr hwnd, IntPtr data);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc proc, IntPtr data);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, System.Text.StringBuilder text, int size);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, System.Text.StringBuilder text, int size);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
  public static IntPtr Find(int pid, string name, bool title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((hwnd, data) => {
      uint owner;
      GetWindowThreadProcessId(hwnd, out owner);
      if (owner != pid) return true;
      var text = new System.Text.StringBuilder(128);
      if (title) GetWindowText(hwnd, text, text.Capacity); else GetClassName(hwnd, text, text.Capacity);
      if (text.ToString() != name) return true;
      found = hwnd;
      return false;
    }, IntPtr.Zero);
    return found;
  }
}
'@
$null = [FixtureWindows]::SetThreadDpiAwarenessContext([IntPtr](-4))
$streamProcess = Get-CimInstance Win32_Process -Filter "Name='Moonlight.exe' AND ParentProcessId=$AppId"
$stream = [FixtureWindows]::Find($streamProcess.ProcessId, 'SDL_app', $false)
$title = 'LL' + [char]0x8FDC + [char]0x7A0B + ' ' + [char]0xB7 + ' ' + [char]0x672C + [char]0x6B21 + [char]0x8FDE + [char]0x63A5
$overlay = [FixtureWindows]::Find($AppId, $title, $true)
$owner = [uint32]0
$null = [FixtureWindows]::GetWindowThreadProcessId($overlay, [ref]$owner)
if (!$stream -or !$overlay -or $owner -ne $AppId) { throw "Missing fixture windows. Stream handle $stream, overlay handle $overlay, overlay owner $owner, app $AppId." }
if ($MoveBy -ne 0) {
  $original = New-Object FixtureWindows+Rect
  $null = [FixtureWindows]::GetWindowRect($stream, [ref]$original)
  if (![FixtureWindows]::SetWindowPos($stream, [IntPtr]::Zero, $original.Left + $MoveBy, $original.Top, 0, 0, 21)) { throw 'Could not move the fixture window' }
  Start-Sleep -Milliseconds 300
}
$client = New-Object FixtureWindows+Rect
$point = New-Object FixtureWindows+Point
$ball = New-Object FixtureWindows+Rect
$null = [FixtureWindows]::GetClientRect($stream, [ref]$client)
$null = [FixtureWindows]::ClientToScreen($stream, [ref]$point)
$null = [FixtureWindows]::GetWindowRect($overlay, [ref]$ball)
@{stream = @($point.X, $point.Y, ($point.X + $client.Right), ($point.Y + $client.Bottom)); ball = @($ball.Left, $ball.Top, $ball.Right, $ball.Bottom)} | ConvertTo-Json -Compress
