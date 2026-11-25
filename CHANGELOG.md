# What's new in FontCraft

## 1.3.5

### Right-to-left, reworked
* Fonts that have Latin letters now show your sample text like every other font, aligned as you chose – also fonts
  made for Arabic or Hebrew. Only a font that can't show the sample text (no Latin letters) shows its own script.
* New **RTL** switch in the bar above the fonts, next to the alignment button (and separate from it). Switched on,
  every font with Arabic, Hebrew or another right-to-left script shows that script first, set from the right. Fonts
  without one keep the sample text as it is – left to right – but aligned to the right.
* Text you type in a right-to-left script is always shown as typed, from the right. Printing a collection follows
  the same rules.
* A line too long for the window now keeps its beginning in view, also when it is aligned right or centred.

### Clean Application Font Caches: more programs, and it scrolls
* Two more choices: **Affinity** (only files named as font caches – your font substitutions, favourite glyphs and
  the fonts Affinity added stay) and the **Craft apps** (Photocraft, VectorCraft, DesignCraft…: the font previews
  they share, fetched again when needed).
* The window no longer grows taller than FontCraft: the list of programs scrolls, and each "What is deleted" list
  scrolls in its own box, so the buttons are always in reach.

### Fixed
* Printed glyph overview: glyphs with tall accents (Ǻ, stacked Vietnamese marks, Greek capitals with breathings) were
  cut off at the top of their box. The boxes are now as tall as the font's tallest glyphs, and every glyph sits
  inside its box on a common baseline.
* Print sheet: a long font name at a large size no longer loses its size ("Playfair Display 60pt" doesn't fit
  the page) – the size is then printed in the margin.
* Tiles are tall enough again for at least two whole lines of the preview.
* Without font folders and with the installed fonts hidden, FontCraft now opens on All instead of an empty list.
* New screenshots on the project page.

## 1.3.4

### Print a font: a one-page specimen
* The font panel on the right has a new **Print** tab. It makes one A4 page of the chosen style: the name large, a
  waterfall from 6 to 72 pt ("Tahoma 6pt" … "Tahoma 72pt"), the characters, a few paragraphs of lorem ipsum and your
  sample text. Every size is printed at exactly that size.
* Choose what goes on the page: the waterfall with the font's name or with the sample text, your own sizes, the
  sample text's size, and how many paragraphs of running text. "Print…" opens the page in your PDF app; "Save as
  PDF…" keeps it. If a choice doesn't fit on the one page, FontCraft leaves out the largest sizes and says so.
* Collection printing now uses exact type sizes too.

### Print the glyph overview
* "Print all glyphs at 12 pt" in the Glyphs tab (and in the Print tab): every glyph of the style, all at one size
  (12 pt unless you change it), each with its code, grouped by Unicode block like on screen – on as many pages as it
  takes. "Print…" opens it in your PDF app; "Save as PDF…" keeps it.

### Clean Application Font Caches…
* New in the FontCraft menu. When a font is missing from a program's font menu, shows up twice or looks wrong, clear
  that program's font cache: **Adobe apps** (Photoshop, Illustrator, InDesign, Acrobat…), **Microsoft Office** and
  **QuarkXPress**, and on a Mac or Linux also your own system font cache. Choose which ones; FontCraft shows what it
  found and how big it is, and tells you to close a program that is still open.
* Only caches are deleted – never fonts, Adobe Fonts, documents or InDesign's recovery data. Each program builds its
  cache again the next time it starts.

### The font list as before, the details in the font panel
* The columns from 1.3.3 are gone again: each font has its one information line above the preview, as before.
* Everything the columns showed is now in **About this font** at the top of the font panel: format, category,
  designer, foundry, languages, version, the date it was added and how it looks (sans, monoline…).
* Search still also shows matching collections, tags and folders.

### Fixed
* The font panel no longer widens by itself when you open the Print tab. Its width is now only remembered when you
  drag its edge, and a panel that had grown to the maximum this way starts at its normal width again.

## 1.3.3

### Search also finds collections, tags and folders
* While you search, a short line above the fonts shows the one or two best matching collections, tags and folders,
  with how many fonts each has. Click one to open it. When nothing matches, the line isn't shown, and the font list
  itself is sorted just as before.
* Switch it off with the filter button next to the search box › "Collections, tags and folders".

### More columns, your way
* The line above each preview now has columns with a header: Name, Styles, Format, Category, Source and Tags, plus
  Designer, Foundry, Languages, Version and Added (date) if you want them. The previews and the size slider stay as
  they were.
* Drag the edge between two column titles to make a column wider or narrower. The ⋯ at the end of the header (or a
  right-click on it) lets you show or hide columns, change their order and go back to the standard columns.
  FontCraft remembers your choice. Columns that don't fit in a narrow window are left out until there's room.

## 1.3.2

### New: make a new font folder from inside FontCraft
* The + next to Folders opens a small menu: **a new folder in your standard font folder**, **a new folder
  anywhere** (or add an existing one), or **a network folder**. FontCraft creates the folder on your disk and makes it
  a font folder right away. Right-click a folder › "New subfolder…" makes one inside it.
* Mark your main font folder as **standard** with the star in Settings › Library & folders (or right-click it). It
  shows a ★ in the sidebar.
* Folders without fonts now show in the folder tree too, with 0, also sub-folders of your font folders. Only font
  files (.ttf, .otf, .ttc, .otc) count – any other files in a folder are ignored, and hidden folders are left out.
* An empty folder says so and offers to open it, so you can copy fonts in; they appear within seconds.

### Settings inside the main window
* Settings no longer float above the app: they open in the main area of the window, where the fonts are, with the
  list of sections on the left. The sidebar stays where it is – choosing something in it, "Back to the fonts",
  the gear button or Esc returns to the fonts.

### Switch off online sources you don't use
* Settings › Library & folders has a switch for each free online source – Google Fonts, Fontshare and Font
  Squirrel. A source that is off disappears from the sidebar; with all three off, the whole Providers section is
  gone. The Font finder and auto-activation then don't use it either. Fonts you already got from it stay in your
  library.

### Duplicates shown more precisely
* A font's details now list only the styles that really are in more than one file, each with its files: the
  original first, the copies marked "duplicate". The family's other styles are no longer listed there.

### Font finder: more fonts, every style
* The Font finder now also compares the 100 Fontshare families (fetched once, then kept on your computer).
* Lettering is often set in a light, bold, black or italic style. After the first pass, FontCraft now fetches all
  styles of the 30 closest free families (Google Fonts and Fontshare) and compares those too – so "Roboto Slab
  Extra Bold" is found as exactly that, not just as "Roboto Slab". Only the few letters it compares are
  downloaded, and only the first time.
* Results show where a match comes from (your library, this computer, Google Fonts or Fontshare) and its style;
  "Get font" works for Fontshare families as well.

## 1.3.1

### New: right-to-left fonts (Arabic, Hebrew, Persian, Urdu…)
* FontCraft recognises fonts made for a right-to-left script: from the design languages the font lists itself
  (its `meta` table), from its letters (right-to-left letters but no Latin ones), or from its name ("… Arabic",
  "… Naskh", "… Hebrew"…). For Google Fonts the list's own script information is used.
* These fonts show a sample in their own script, aligned to the right and written from right to left, with the
  letters joined the way the script needs (FontCraft now shapes text with the HarfBuzz rules). Alignment follows:
  "align left" means the start of the line, which is on the right for these scripts.
* Type Arabic or Hebrew into the preview field at the bottom and every font shows it that way; mixed
  right-to-left and left-to-right text comes out in the right order. The field sits on the right for such
  text, and the Samples menu has Arabic, Persian, Urdu, Hebrew and mixed samples. Joined Indic scripts
  (Devanagari, Bengali, Tamil…) are shaped, too.
* Printed PDFs show right-to-left fonts the same way.
* The first start after the update notes the writing direction of every font once (a large library takes a
  minute or two).

### Fixed: "Package" on the Mac keeps the fonts' own file names
* On macOS (and Linux) active fonts now appear in your font folder under their own file names, as on Windows,
  so "Package" in Illustrator, InDesign and other apps copies them as `Font-Bold.otf` instead of
  `FontCraft-…-Font-Bold.otf`. The links with the long names from earlier versions are replaced the first time
  FontCraft starts after the update. Fonts you installed yourself are never touched.

### Fixed: "Import as a duplicate" really makes a duplicate
* When the very same file was already in the import folder, FontCraft reported "imported as duplicate" but didn't
  copy anything. Now there is always a second file ("Font (2).ttf").
* Duplicates are no longer hidden: a family with the same style in more than one file shows "1 duplicate" in the
  list, and its details list every file with a button to show it in its folder. A folder that only holds a
  duplicate now shows the right family.

## 1.3.0

### List and tiles
* The two buttons at the top switch between the **list** and a **tile view**. All tiles have the same size;
  how wide they are and how many fit in a row follow the window's width and the preview size, so a big window
  or screen shows more tiles. The tile view now also works for Google Fonts, Fontshare and Font Squirrel.
* Tiles only show the preview lines that fit completely, no half-cut line at the bottom.

### Text alignment
* The alignment button opens a menu with all options, each with its icon and name: align left, centre,
  align right, and justified with the last line left, centred or right. The button's icon shows the one in use.
* A "centred" setting from an earlier version is kept.

### Adjustable panels
* Drag the edge of the sidebar or of the font details panel to make it wider or narrower. FontCraft remembers
  both widths; Settings › Appearance can put them back to normal.
* Scroll bars stay put: a long list always shows its scroll bar, a short one (a folder with one font) shows
  none. The floating bars that grew under the mouse are gone, and with them the flicker while dragging the
  edge of the details panel.

### New: print a collection
* Right-click a collection (or a tag or smart filter) and choose **Print…**: FontCraft makes a PDF with every font in
  it – its name and your preview text in the font, at the current preview size, colours and alignment – and opens
  it in your PDF app, where you can print it with the usual print dialog or save it. **Save as PDF…** asks where
  to keep it first. Each font stays together on one page; pages are numbered.
* The PDF contains pictures of the previews, never the font files themselves, so it can be shared safely.
* The old browser specimen with every style is still there as "Specimen sheet (every style)…".

### New: importing fonts that are already in the library
* When you import a font that FontCraft already has (same PostScript name), it asks what to do: import it as a
  duplicate, use the font already in the library, replace it with the new one, or skip it. A font imported again
  from the very same file can be used as it is, read again, or skipped. "Remember my choice" stops the question.
* "Replace" only replaces FontCraft's own copies in the import folder – your own font files elsewhere are never
  deleted; the new font is then added next to them.
* New page **Settings › Import**: activate imported fonts, show a report after importing (what was imported,
  duplicated, replaced, skipped or couldn't be read), put imported fonts into a new "Imported <date>" collection,
  and choose what happens with folders you drop or import – added where they are, or copied into the import
  folder together or with their folder structure. Plus the two "don't ask" settings with their default answers.
* "Import a folder…" in the Add menu.

### New: Smart filters
* Save a search as a **smart filter** in the sidebar (Smart filters › +): give it a name and rules such as
  "Languages contains Arabic", "Category is Sans Serif" and "Activation is Activated", for all or any of them.
* Rules can look at languages (17 writing systems, from Latin to Arabic, Devanagari, Chinese, Japanese and
  Korean), category, activation, name, designer or foundry, tags, collections, format, variable fonts,
  favorites, where a font comes from, the number of styles and when it was added.
* The window shows how many families match while you build it. Right-click a smart filter to edit, duplicate,
  switch all its fonts on or off, print it or delete it; double-click edits it, too.
* FontCraft notes the languages of every font the first time it starts after the update (quickly, without reading
  the fonts again in full).

### Hide the sidebar
* When the mouse is over the sidebar, an arrow appears in its top-right corner, above "All". Click it to hide
  the sidebar and give the fonts the whole width.
* While it is hidden, move the mouse to the left edge of the window and the sidebar slides out over the list
  (handy for dragging fonts onto a collection); it goes away when the mouse leaves. Its arrow keeps it open
  again. Ctrl+B (⌘B) and the button at the top of the left bar still show and hide it, too.

### Keyboard shortcuts
* Ctrl+Shift+A activates and Ctrl+Shift+D deactivates the selected fonts (⌘⇧A / ⌘⇧D on a Mac), plus shortcuts
  for favorites, delete, rename, copy name, show in folder, print, list/tile view, preview size (Ctrl + / − / 0),
  sidebar, theme, search, Font finder, add folder, import, new collection or tag, rescan, settings (Ctrl+,) and
  arrow keys, Home and End to move through the list – in the tile view up and down go a whole row, and the list
  scrolls along.
* Settings › Keyboard shortcuts lists them all with the key names of your keyboard (for example Strg and
  Umschalt on a German Windows keyboard, ⌘ and ⇧ on a Mac). F1 opens that list. The right-click menu shows the
  shortcut next to each entry.

### A roomier settings window
* Settings is bigger and has a list of sections on the left – Appearance, Font list, Library & folders, Import,
  Activation, Start-up & tray, Auto-activation, Sync, Keyboard shortcuts and About – so related settings sit
  together.

### Fixed: "Package" in Illustrator, InDesign and other apps now includes activated fonts
* Collecting fonts into a package only worked for system fonts, because these apps only look in the Windows
  font folders. On Windows, an active font is now placed in your personal font folder and font list – just
  like a font installed "for me only", under exactly the name Windows itself would use (Illustrator finds
  the file through that name) – so it is copied into the package with its own file name. Fonts on the same
  drive are only linked there (no extra space); fonts from another drive or a network folder are copied while
  they are active. Deactivating removes them again, and FontCraft tidies up leftovers every time it starts.
  Restart the Adobe app once after updating.
* On macOS and Linux, activated fonts now appear as the font file itself (a hard link) instead of a shortcut
  whenever the font is on the same drive, so packaging copies the real font.

## 1.2.1

### A new face for FontCraft
* A new app icon: the platypus. It shows up on the window, the taskbar and the tray, on FontCraft.exe, in the
  Mac app and in the Linux app menu.

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
