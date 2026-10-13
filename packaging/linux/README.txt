Orphion for Linux
=================

Control QCY earbuds (noise cancelling, transparency, equalizer, touch
controls, dual device, LDAC, wear detection and more) without the phone app.

Requirements
------------
- A 64-bit (x86_64) distribution with glibc 2.31 or newer: Ubuntu 20.04+,
  Debian 11+, Fedora 32+, RHEL/Rocky/Alma 9, openSUSE Leap 15.3+, Linux Mint
  20+, Arch, Manjaro and their derivatives
- BlueZ (the standard Linux Bluetooth service) running, adapter on
- For the window: any Wayland or X11 desktop with OpenGL (GNOME, KDE Plasma,
  Xfce, Cinnamon, MATE, Budgie, LXQt, …)
- For the tray icon: a StatusNotifierItem host. Built in on KDE Plasma,
  Ubuntu's GNOME, Cinnamon, Budgie and LXQt; on Xfce add the "Status Notifier
  Plugin" to the panel; on plain GNOME install the "AppIndicator and
  KStatusNotifierItem Support" extension.

Install
-------
    ./packaging/linux/install.sh              # into ~/.local, with a menu entry
    ./packaging/linux/install.sh --autostart  # also start the tray icon at login

The programs also run directly from bin/ without installing.

Programs
--------
bin/orphion       The control panel. It also starts a tray icon with battery
                  levels and quick controls, which stays after the window is
                  closed (turn off under Settings -> Appearance, or run
                  `orphion --no-tray`). `orphion --tray` starts only the tray.
bin/orphionctl    Command line, e.g.  orphionctl anc   (lists your model's noise modes)
                  Run `orphionctl --help` for everything it can do.

Tips
----
- Pair the earbuds for audio as usual, then take them out of the case.
- Close the QCY app on your phone: the earbuds accept only one control
  connection at a time.
- Something wrong? Run `orphionctl -vv status` and keep the output.

Uninstall
---------
    ./packaging/linux/uninstall.sh

Firmware updates are intentionally not supported.
