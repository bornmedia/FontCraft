# Putting FontCraft on GitHub

You only do the first part once. After that, each new version takes two clicks.

## 1. Create the project (once)

1. Install **GitHub Desktop** from https://desktop.github.com and sign in with your GitHub account.
2. In GitHub Desktop choose **File › Add local repository…** and pick this folder (the one this file is in).
3. GitHub Desktop says the folder isn't a repository yet. Click **create a repository**, keep the name
   `FontCraft`, and click **Create repository**. Leave the licence and git-ignore choices on "None",
   because both files already exist.
4. Click **Publish repository**. **Untick "Keep this code private"**, or other people won't be able to download it.

## 2. Make a release (every new version)

1. Open your project on github.com and click the **Actions** tab. If GitHub asks, allow workflows to run.
2. Click **Release** on the left, then **Run workflow › Run workflow**.
3. Wait about 10–15 minutes until the run shows a green tick. Your **Releases** page now has a download
   for each system.

Share this link, which always points to the newest version:

    https://github.com/YOUR-NAME/FontCraft/releases/latest

## 3. Releasing a newer version later

1. Change the version number at the top of `Cargo.toml` (for example `version = "1.1.0"`).
   You can also ask Craft Anything to do it.
2. In GitHub Desktop write a short summary (for example "Version 1.1.0"), click **Commit to main**,
   then **Push origin**.
3. Run the **Release** workflow again (step 2 above).

Everyone with FontCraft sees a "FontCraft 1.1.0 is available" notice at the bottom of the window.

## No fonts in the project

FontCraft doesn't contain a single font file. The window uses the computer's own interface font, and
Google Fonts are only downloaded after installing, when someone asks for them. Three safety nets keep it that way:

* `.gitignore` stops GitHub Desktop from offering any font file (`.ttf`, `.otf`, `.ttc`, `.woff`…) for upload.
* The **Release** workflow stops with an error if a font file is in the project or in a download.
* The window toolkit's built-in fonts are switched off in `Cargo.toml`.

**If fonts already reached GitHub:** deleting them in a new commit isn't enough, because GitHub keeps every
earlier version. On github.com open the repository › **Settings** › (bottom) **Delete this repository**. Then,
in GitHub Desktop, remove the repository (**Repository › Remove…**), add the `app` folder again and publish it fresh.
Make sure you pick the **app** folder, not a folder that contains your fonts.

## Good to know

* The Mac downloads aren't signed with a paid Apple Developer certificate (99 $/year), so on the first start
  people right-click › *Open*. Windows may show a SmartScreen warning for the same reason.
* Only the files in this folder go to GitHub. Build output and the settings that only apply to this PC
  are left out automatically (see `.gitignore`).
