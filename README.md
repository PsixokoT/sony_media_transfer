# Sony Transfer

English · [Русский](README.ru.md)

A Windows 11 app that copies photos and videos from a Sony A7 IV (firmware 6.01) into a library folder on the computer. It shows what is on the cards day by day, marks what has already been copied and puts new shots into dated folders.

## Install

Download `sony-transfer.exe` from the [latest release](https://github.com/PsixokoT/sony_media_transfer/releases/latest) and run it. There is no installer: it is a single portable exe. It needs Windows 11, which already has WebView2. The exe is not signed, so SmartScreen may warn about it: click "More info → Run anyway".

On the camera, turn on Mass Storage mode: `MENU → Setup → USB → USB Connection Mode → MassStorage(MSC)`, and set `USB LUN Setting` to Multi. On first launch, set the library folder in Settings.

## How it works

### Where files come from

- The camera connects in Mass Storage (MSC) mode. MTP (shows only slot 1 and is slow) and the Camera Remote SDK are not used.
- A card is a removable drive that has `DCIM\NNNxxxxx`, `M4ROOT` (CFexpress Type A) or `PRIVATE\M4ROOT` (SD). The PMHOME drive (the camera's internal memory, with a LICENSE folder) and empty slots are skipped. A card reader or a flash drive with the same layout is detected the same way.
- Plugging and unplugging the camera is tracked automatically: every 2 s the app checks which removable drives are cards (folders only, no files are read). Files are read again only when the set of cards changes.

### Files and grouping

- Photos: ARW in slot 1, JPG or HEIF (`.HIF`) in slot 2. Video (XAVC S and XAVC HS): `M4ROOT\CLIP\C0001.MP4` + `C0001M01.XML`, proxy `M4ROOT\SUB\C0001S03.MP4`. On an SD card `M4ROOT` sits inside `PRIVATE\`. `MEDIAPRO.XML` is a service file and is not copied.
- Files from all cards are shown as one list grouped by day. The day is the capture date: EXIF `DateTimeOriginal` for photos, `CreationDate` from the XML for videos. Without metadata, the file date is used. A day ends at midnight.
- Newest first: days go from the latest to the earliest, and within a day the latest shot is at the top left.
- One tile is one shot or clip. ARW, JPG and HIF are linked by name and capture date; MP4, XML and proxy by clip number. If two cards hold the same files, only one is copied.
- The tile caption lists the extensions of its files, plus "proxy" for a clip with a proxy: "MP4 XML proxy".

### Viewing and picking

- Photo previews are the JPEG embedded in the ARW (for a JPG, the 160×120 EXIF thumbnail). The full-screen viewer shows a JPG in full.
- HIF previews are the embedded HEVC images, 1616×1080 and 320×212, decoded by WebView2 through WebCodecs: 320×212 in the grid, the largest one that decodes in the viewer. Not every graphics card decodes HEVC 4:2:2; then the viewer shows 320×212 (it is always 4:2:0). If HEVC doesn't decode at all, the embedded 160×120 JPEG is shown. HIF rotation comes from `irot`.
- There is an "Open in an external program" button. "Previous" and "Next" buttons sit on the sides of the viewer; the first and last tiles don't have them.
- Video: the original plays in WebView2 first. If it won't play, the proxy plays; if there is no proxy, an "Open in the system player" button is shown. "Won't play" means an error or a video without a picture: without a decoder Chromium plays only the sound. Video is served through Tauri's asset protocol, which is open to `M4ROOT` only.
- Keyboard: Enter opens the viewer, ← / → page, Space checks or unchecks, Delete hides or unhides, Esc closes. ← / → also move the cursor in the grid. In the viewer the mouse wheel pages too: down is next, up is previous. The order is the on-screen order: → goes to the next, that is older, tile.
- Selection works like in Lightroom and File Explorer: a click selects a tile, Ctrl+click adds or removes it, Shift+click and Shift+← / → select a range, Ctrl+A selects everything. A rubber band (drag with the left button held, from a tile or from empty space) selects every tile it touches; with Ctrl or Shift it adds them to the selection. A click outside the tiles clears the selection. A double click opens the viewer. Space and the checkbox on a selected tile check or uncheck the whole selection; if the selection has both checked and unchecked tiles, Space checks them all.
- Before a video opens in an external program, the in-app player pauses.

### Hiding rejects

- Rejects (photos and videos) are not deleted but hidden from the list: the file stays on the card. A hidden tile is not copied even if it was checked, and doesn't count toward the "Copy" counter, the totals or the day checkbox.
- Delete in the grid and in the viewer, and a button in the header, hide the selection or the tile under the cursor. It is a toggle, like Space: if all targets are already hidden, they come back. The button reads "Hide" or "Unhide" depending on what it will do. There is no confirmation; hiding clears the selection. In the viewer, hiding moves on to the next (older) visible tile, or to the previous one if there is no next, or closes the viewer if there is neither.
- Hidden tiles are not in the grid, and a day where everything is hidden disappears. If the cards have hidden tiles, a "Show hidden (N)" checkbox appears in the header; it is off on launch. When it is on, hidden tiles stay in place, half-transparent, marked "hidden" and with an "Unhide" button instead of a checkbox; they can be selected (Shift and Ctrl+A included) and unhidden with Delete. The viewer always skips hidden tiles.
- The list is kept in the app: the tile (name + capture time) and the date it was last seen on a card. Every card read updates that date; entries not seen for 30 days (the card was formatted) are dropped on launch. Settings has "Unhide all hidden (N)".
- A clip's time comes from its XML, or from the file date if only the card with the proxy is inserted. The key is different then, and a hidden clip shows up again.

### Already copied

- On every launch the library root is scanned in the background. Only media files count; `.xmp` and `.acr` are skipped. There is no index file.
- Files match by name, size and capture date. The date is needed because `DSC…` numbers start over after 9999. The date comes from the range in the top-level folder name; a file is opened (EXIF, clip XML) only when the day falls outside that range. Already copied tiles are grey and unchecked. If you check one by hand, it is copied again.
- A tile counts as copied when the library has all of its needed files (see the JPG/HIF setting under Copying). If only some were copied, the rest are copied.

### Copying

- Checked tiles are copied, with progress. Nothing is deleted from the card.
- The "Copy JPG/HIF along with RAW" setting is off by default. While it is off, a shot with an ARW needs only the ARW: its JPG and HIF are not copied. A shot without an ARW is copied whole, and a video gets all its files. If you turn the setting on later, JPG and HIF are added to shots copied earlier. The viewer, the tile caption and the day totals show all files on the card.
- Two modes: one folder for the whole range, `YYYY.MM.DD - YYYY.MM.DD - description`, or a folder per day, `YYYY.MM.DD - description`. All files lie together inside the folder.
- If a day falls within the range of an existing library folder, that folder is offered by default, and the other days go to a new folder. The range is read from the date at the start of the name; older names without " - " before the description are recognized too.
- Without a description, a new folder is named just by its date or range. " - " for the description is appended when the field gets focus, unless the field was edited by hand, and trimmed when it loses focus. The first field opens focused. Switching the mode resets manual edits.
- The library folder can be changed for this one copy, the same way as in Settings: type it in or choose it. It is not saved to Settings. After a change the library is checked again and the folder plan is recomputed; copying is disabled until the check finishes. The grey "already copied" tiles still follow the folder from Settings.
- While copying, the window can't be closed: there is no cancel. After "Done", the full path of each folder is shown with an "Open in Explorer" button.
- Each file is first written as `*.part`, then renamed, and then its size is checked. The file date is preserved. Speed comes first, so files go one at a time and without fsync. The library is never overwritten: a file with the same name and size is skipped; with a different size, copying stops with an error.

### Other

- The library root is set in the Settings window: typed in or chosen with the standard folder dialog. It is saved. The header shows it in grey (a long path is truncated, the full one is in the tooltip), next to an "Open in Explorer" button. If no root is set, a "Choose a folder…" button stands in place of the path and opens Settings.
- Popups (Settings, Copying) close with the cross at the top right or with Esc. A click outside a popup doesn't close it.
- Button icons come from the Segoe Fluent Icons font, which ships with Windows 11. The app icon is a memory card with a down arrow; its source is `src-tauri/icons/icon.svg`.
- Languages: Russian and English. The default is the Windows language; it can be switched in Settings. Rust's own error messages use the chosen language; system errors come in the Windows language.
- The "Eject" button safely ejects the USB device of each card, like "Eject" in File Explorer. So all of the camera's drives go (with LUN = Multi, slot 1, slot 2 and PMHOME are one USB device), and all slots of a card reader. If a drive is in use by another program, an error is shown. The camera is not ejected by itself after copying.
- There is no button to format a card: format only in the camera. Sony doesn't guarantee that a card formatted on a computer works, and for CFexpress Type A there is no way to do the same on a computer.

## Development

Tauri 2: logic in Rust (`src-tauri/`), UI in TypeScript + React + Vite (`src/`). Rust stable and Node 24 are pinned in `mise.toml`; MSVC Build Tools (C++ and Windows SDK) are installed with winget. The build is a portable exe, with no installer or auto-update.

```sh
mise install                # Rust and Node from mise.toml
npm install
npm run tauri dev           # run with hot reload
npm test                    # Rust unit tests
npm run tauri build         # src-tauri\target\release\sony-transfer.exe
npm run tauri icon src-tauri/icons/icon.svg  # icons from the SVG; keep only the files listed in bundle.icon
                            # the exe picks up new icons only if tauri.conf.json changes (or after cargo clean)
```
