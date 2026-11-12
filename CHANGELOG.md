# What's new in FontCraft

## 1.2.5

### A new face for FontCraft
* A new app icon: the platypus. It shows up on the window, the taskbar and the tray, on FontCraft.exe, in the
  Mac app and in the Linux app menu.

### Fixed: "Package" in Illustrator, InDesign and other apps now includes activated fonts
* Collecting fonts into a package only worked for system fonts, because these apps only look in the Windows
  font folders. On Windows, an active font is now placed in your personal font folder and font list – just
  like a font installed "for me only", under exactly the name Windows itself would use (Illustrator finds
  the file through that name) – so it is copied into the package with its own file name. Fonts on the same drive are only
  linked there (no extra space); fonts from another drive or a network folder are copied while they are
  active. Deactivating removes them again, and FontCraft tidies up leftovers every time it starts.
  Restart the Adobe app once after updating.
* On macOS and Linux, activated fonts now appear as the font file itself (a hard link) instead of a shortcut
  whenever the font is on the same drive, so packaging copies the real font.

### New theme: Sand
* A dark theme with the warm sand tone of the icon for highlights: active dots, sliders, buttons and selections.
  Choose it in Settings › Colour theme, or with the theme button on the left.

## 1.2.0

### New: delete fonts from inside FontCraft
* Right-click a family (or select several and press **Delete**, or use "Delete…" in the selection bar).
  FontCraft asks first, with its own warning for each kind of font:
  * **Online sources** (Google Fonts, Fontshare, Font Squirrel): the fonts are deactivated and FontCraft's local
    copy is deleted. They stay in the online list; activating them again downloads them again.
  * **Your folders**: the files are deleted from disk for good – not to the Recycle Bin/Trash. FontCraft says
    so and asks whether you're sure; it also warns when the folder is on the network and shared with others.
  * **System fonts**: instead of deleting them you can **move** them into a folder of your library. The folder is
    filled in for you (the one you used last time, or a folder you already have); a new folder is added to
    FontCraft's Folders automatically. Fonts the operating system needs itself are marked and left out unless
    you untick the box, and FontCraft explains what can go wrong on Windows, macOS or Linux. Your system asks
    for administrator permission when it's needed; a font Windows is still using goes at the next restart.

### New: network folders
* "Add network folder…" (in the Add menu and under Folders): type or paste `\\server\share\fonts`, a mapped
  drive, or `smb://server/fonts` on macOS and Linux – or browse to it.
* Network folders are marked in the sidebar. When the network is away the folder shows "offline", its active
  fonts are remembered and switched on again as soon as it's back. FontCraft looks at network folders every
  30 seconds instead of every 2, to keep the network quiet.

### Search
* A little × in the search bar clears it in one click (Esc in the box does the same).

### Font finder: letters that touch
* Two or three letters that run into each other (ink bleed, tight spacing, blurry photos) are now cut apart
  automatically where they meet in a thin joint. Wide single letters like m, w, M, W and H stay whole.
* Right-click any letter box to cut it into 2–12 letters, join it with the letter on its right (for a letter
  that broke in two), or leave it out. "Undo all my cuts and joins" brings back the original boxes.
* What you left out and what you typed stays when you cut or join boxes.

## 1.1.0

### New: Font finder – which font is in this picture?
* Drop, open or paste (Ctrl+V) a screenshot, a photo of a poster or a logo. FontCraft finds the letters and
  compares them with every font in your library, the fonts on your computer and all of Google Fonts, closest first.
* Drag a box around the text you care about, click a letter to leave it out, or type the text you see to fix
  misread letters (this works best for joined-up script).
* Activate a match with one click, or get a free Google family straight into your library.
* Everything happens on your computer; the picture is never uploaded.
* Open a picture with FontCraft ("Open with…"), drop it onto the window, or start FontCraft with `--finder`.

### New: Fontshare and Font Squirrel
* **Fontshare**: about 100 free families from the Indian Type Foundry, with live previews, filters and
  one-click download and activation, just like Google Fonts.
* **Font Squirrel**: about 1,000 hand-picked families that are free for commercial use. Clicking the dot opens
  the download in your browser; FontCraft notices the file in your Downloads folder (even if you moved that
  folder), adds the family and activates it.
* Auto-activation can now fetch missing document fonts from all three free sources.
* "Most popular" sorting works for every online source.

### Improvements
* Fonts from Fontshare and Font Squirrel show where they came from in the details panel.
* Dropping a picture anywhere on the window opens it in the Font finder.
* Tidier code under the hood: no compiler warnings on Windows, macOS or Linux.

## 1.0.0

The first release: library with folders, collections and tags, one-click activation on Windows, macOS and Linux,
Google Fonts, Super search, glyphs, playground, waterfall, type tester, variable-font sliders, five themes,
auto-activation for documents, sync through cloud drives, autostart and the system tray.
