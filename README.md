# imgvw
Image viewer in rust.

Use **File > Open Folder(s)** to choose an image folder (one folder at a time),
**File > Preferences** to configure the app, or **File > Exit** to close it.
Use Ctrl+O to open a folder.
Opening a folder includes supported images from all of its subfolders.
Symbolic links are not followed. Images are sorted by filename (case-insensitive),
with the full path breaking ties; matching filenames in different folders are
kept as separate images. Folder counts include all discovered images. An
unreadable subfolder reports an error rather than silently showing a partial list.
While scanning, the folder heading shows a live count of files checked and images
found across all subfolders. The file count includes non-image files.
The top bar contains the File menu with larger menu text.
The folder heading shows the folder name and total image count in either view.
Menus, panels, Preferences, the photo grid, and the preview canvas share the
same dark gray background.
Buttons outside the menu bar and its dropdowns use a lighter gray fill, with
distinct hover, pressed, and selected states.

Photos open in a dark, full-window thumbnail grid. Adjust the thumbnail size
with the slider at the bottom right. Click a photo to select it; double-click
or press Enter to open its preview and EXIF details. Use **Photos** or Escape
to return to the grid. Left/Right navigate between photos in either view.
In Preview, the bottom-right slider controls **Zoom** (25% to 400%) instead
of thumbnail size. 100% is the fitted preview size; scroll or grab and drag
with the left mouse button to explore enlarged images. The cursor changes to
an open hand over a pannable image and a closed hand while dragging.
Zoom and thumbnail size are kept separately when
switching views.

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

## Source layout

- [src/main.rs](src/main.rs): native window setup and app startup.
- [src/viewer/mod.rs](src/viewer/mod.rs): viewer state, navigation, loading events,
  texture cache, and frame orchestration.
- [src/viewer/controls.rs](src/viewer/controls.rs): menu bar, folder heading, and
  view/size controls.
- [src/viewer/preferences.rs](src/viewer/preferences.rs): Preferences UI and
  applying saved settings.
- [src/viewer/thumbnails.rs](src/viewer/thumbnails.rs): grid, filmstrip, and
  shared thumbnail rendering.
- [src/viewer/preview.rs](src/viewer/preview.rs): preview rendering, zoom/panning,
  empty states, and image/EXIF details.
- [src/viewer/theme.rs](src/viewer/theme.rs): shared colors, menu font size, and
  theme setup.
- [src/viewer/tests.rs](src/viewer/tests.rs): headless UI and interaction tests.
- [src/loader.rs](src/loader.rs): background folder scanning and image decoding.
- [src/settings.rs](src/settings.rs): settings format and atomic persistence.
