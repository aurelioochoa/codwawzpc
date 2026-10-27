# Portable codwawzpc

Extract the archive to a writable folder, then open `Launch` (or run `./Launch` in a terminal). The folder can be moved while the launcher is closed. Import your own Call of Duty: Zombies iPhone 1.5.0 IPA with the launcher; game files are not included or downloaded. Press Escape, Enter, Space or left-click to skip the intro movie. The installation starts with empty data, regardless of any existing source installation.

Optionally run `./Launch --install-desktop` to add the launcher to the desktop menu. The entry contains the current folder path; run that command again after moving the folder.

The `userdata` folder holds imported files, settings, saves, logs and achievement captures. Back it up before updating. To update, extract the new release to a new folder and copy `userdata` into it. To transfer a save from an existing installation, use the old launcher's Export saved game and this launcher's Import saved game. Keep the original as a backup.

In Settings & controls, edit each action's comma-separated input names. Examples: `Key:W`, `Key:Left Shift`, `Mouse:Left`, `Mouse:Wheel`, `Pad:A`, `Pad:RightTrigger`. A conflict prevents saving. Reset defaults restores all settings; Close cancels unsaved edits. Controller sticks can be normal or swapped. Escape, Tab, F10 and F11 and menu navigation remain fixed.

Supported baseline: Linux x86_64 with glibc 2.35 or newer (such as Ubuntu 22.04), a graphical X11 desktop or XWayland, a working OpenGL driver, and an audio device and desktop audio service. Python, Tk, Pillow and FFmpeg are bundled. The folder must be writable. Host graphics drivers and the system C/C++ runtime remain necessary.

License notices are in `LICENSE`, `NOTICE.md`, `licenses/`, and the resource directories. Ubuntu package versions are listed in `licenses/packages.txt`; corresponding Ubuntu source packages can be obtained from the Ubuntu archive using `apt source <package>=<version>` with source repositories enabled. FFmpeg includes GPL components. The game IPA is supplied by the user.
