# imgvw
Image viewer in rust.

Use **File > Open Folder(s)** to choose an image folder (one folder at a time),
**File > Preferences** to configure the app, or **File > Exit** to close it.
Use Ctrl+O to open a folder.
The top bar contains the File menu with larger menu text.
Menus, panels, Preferences, the photo grid, and the preview canvas share the
same dark gray background.

Photos open in a dark, full-window thumbnail grid. Adjust the thumbnail size
with the slider at the bottom right. Click a photo to select it; double-click
or press Enter to open its preview and EXIF details. Use **Photos** or Escape
to return to the grid. Left/Right navigate between photos in either view.

## Preferences

- **Default folder location**: type a folder path or use **Browse...** to choose
  one. This folder opens automatically at startup and is the initial location
  of the Open Folder(s) dialog. Leave it empty to start without a folder.
- **Default view**: choose **Photos grid** or **Preview**. Saving applies this
  view immediately; it is also used at startup and whenever a folder is opened.
- **Save** saves the settings and keeps Preferences open. **Ok** also saves any
  changes, then closes the window. Closing with the window's X discards unsaved
  edits. Errors remain visible and prevent Ok from closing the window.

Settings are stored per user in `imgvw/settings.json` under the operating
system's configuration directory (`%APPDATA%\imgvw\settings.json` on Windows,
`$XDG_CONFIG_HOME/imgvw/settings.json` or `~/.config/imgvw/settings.json` on Linux,
and `~/Library/Application Support/imgvw/settings.json` on macOS).
