# viewr
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
In Preview, hover over the bottom thumbnail strip and use the mouse wheel to
scroll through thumbnails horizontally. Wheel down moves toward later photos;
wheel up moves toward earlier photos. Horizontal trackpad scrolling also works.
Hover over a grid or filmstrip thumbnail to show its filename along the bottom
edge. Long names are truncated to fit. The main preview image has no hover caption.
In Preview, the bottom-right slider controls **Zoom** (25% to 400%) instead
of thumbnail size. 100% is the fitted preview size. Hover over or click the image
and use the mouse wheel to zoom: wheel up zooms in, wheel down zooms out, within
the same 25% to 400% range. Grab and drag with the left mouse button to explore
enlarged images. The cursor changes to
an open hand over a pannable image and a closed hand while dragging.
Zoom and thumbnail size are kept separately when
switching views. Selecting a different image resets zoom to 100% (fitted) by
default; enable **Retain zoom percentage between images** in Preferences to
keep the current zoom instead.

## Image details

In Preview, the resizable right-hand side panel starts with **IMAGE DETAILS**
at the top. It shows the selected image's filename, full path, pixel dimensions,
file size, and format. The panel scrolls vertically when its contents exceed
the available height.

Expand **EXIF** to view metadata tags; this section is collapsed by default.
Images without EXIF metadata show a message when the section is expanded.

## Preferences

Preferences opens centered in the main window, with a wider folder field and
spaced controls. The dialog can still be dragged to another position; reopening
it centers it again.
Save and Ok use matching sizes and align with the bottom-right content edge.

- **Default folder location**: type a folder path or use **Browse...** to choose
  one. This folder opens automatically at startup and is the initial location
  of the Open Folder(s) dialog. Leave it empty to start without a folder.
- **Default view**: choose **Photos grid** or **Preview**. Changing this setting
  and saving applies the new view immediately; it is also used at startup and
  whenever a folder is opened. Saving other settings preserves the current view.
- **Retain zoom percentage between images**: unchecked by default. Enable to
  keep the current preview zoom when clicking another thumbnail or navigating
  with Left/Right. When unchecked, a different image opens at 100% (fitted).
- **Save** saves the settings and keeps Preferences open. **Ok** also saves any
  changes, then closes the window. Closing with the window's X discards unsaved
  edits. Errors remain visible and prevent Ok from closing the window.

Settings are stored per user in `viewr/settings.json` under the operating
system's configuration directory (`%APPDATA%\viewr\settings.json` on Windows,
`$XDG_CONFIG_HOME/viewr/settings.json` or `~/.config/viewr/settings.json` on Linux,
and `~/Library/Application Support/viewr/settings.json` on macOS).
If this file does not exist, viewr loads preferences from the old
`imgvw/settings.json` location in the same configuration directory.
Saving preferences writes to the new location and leaves the old file unchanged.

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
- [src/viewer/overlays.rs](src/viewer/overlays.rs): shared filename captions.
- [src/viewer/theme.rs](src/viewer/theme.rs): shared colors, menu font size, and
  theme setup.
- [src/viewer/tests.rs](src/viewer/tests.rs): headless UI and interaction tests.
- [src/loader.rs](src/loader.rs): background folder scanning and image decoding.
- [src/settings.rs](src/settings.rs): settings format and atomic persistence.
