Orphion for Windows
===================

Control QCY earbuds (noise cancelling, transparency, equalizer, touch
controls, dual device, LDAC, wear detection and more) without the phone app.

Requirements
------------
- Windows 10 version 2004 or newer, or Windows 11
- A Bluetooth adapter with Bluetooth Low Energy (practically every laptop)
- Nothing else: no runtimes or drivers to install

Install
-------
Right-click install.ps1 -> "Run with PowerShell", or in a terminal:

    powershell -ExecutionPolicy Bypass -File install.ps1 -Autostart

-Autostart starts the tray icon when you sign in (and right away);
-AddToPath makes the `orphionctl` command line tool available in new
terminals. You can also simply run the .exe files from this folder without
installing.

Programs
--------
orphion.exe       The control panel. It also starts a tray icon: battery
                  levels in the tooltip, quick noise control, switches and EQ
                  presets in the menu; click it to open the control panel.
                  The tray stays after the window is closed (turn off under
                  Settings -> Appearance, or run `orphion --no-tray`).
                  `orphion --tray` starts only the tray.
orphionctl.exe    Command line, e.g.  orphionctl anc   (lists your model's noise modes)
                  Run `orphionctl --help` for everything it can do.

Tips
----
- Pair the earbuds with Windows for audio as usual, then take them out of
  the case.
- Close the QCY app on your phone first: the earbuds accept only one control
  connection at a time.
- Turning on LDAC or dual device connection restarts the earbuds; the apps
  reconnect by themselves.
- Something wrong? Run `orphionctl -vv status` in a terminal and keep the output.

Uninstall
---------
    powershell -ExecutionPolicy Bypass -File uninstall.ps1

Firmware updates are intentionally not supported.
