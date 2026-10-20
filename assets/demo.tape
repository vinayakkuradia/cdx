# cdx demo: ~25 seconds that show what makes cdx worth installing.
#
#   1. Browse and jump         cdx            (arrows or h j k l, Space to go)
#   2. Favourites, speed dial  b, cdx 1, cdx docs
#   3. Filter as you type      0
#   4. Still just cd           cdx .., cdx -, cdx ~
#
# Record it (from the repository root) with VHS: https://github.com/charmbracelet/vhs
#
#   cargo build --release
#   PATH="$PWD/target/release:$PATH" vhs assets/demo.tape
#
# That writes assets/demo.gif. The folders it browses are made by scripts/demo-setup.sh
# inside a temporary home, so your own favourites and config are never touched.

Output assets/demo.gif

Require cdx-bin
Require bash

# Boost responsiveness and framerate
Set Framerate 60
Set PlaybackSpeed 1.3

Set Shell "bash"
Set FontSize 20
Set Width 1100
Set Height 620
Set Padding 24
Set Theme "Catppuccin Macchiato"
Set WindowBar Colorful
Set BorderRadius 10
Set Framerate 24
Set TypingSpeed 45ms

# Set the scene off camera: pretend home, sample folders, the shell wrapper.
Hide
Type "source scripts/demo-setup.sh"
Enter
Type "clear"
Enter
Sleep 500ms
Show

# ---- 1. Browse, then jump ----------------------------------------------------
Type "# browse, then jump using space"
Sleep 500ms
Enter
Type "cdx"
Sleep 800ms
Enter
Sleep 900ms
Right
Sleep 600ms
Down@250ms 4
Sleep 700ms
Space
Sleep 1200ms

# ---- 2. Favourites and speed dial --------------------------------------------
Ctrl+L
Type "# bookmark with b, then speed dial"
Sleep 500ms
Enter
Type "cdx"
Sleep 500ms
Enter
Sleep 800ms
Left
Sleep 500ms
Up@250ms 2
Sleep 500ms
Type "b"
Sleep 1500ms
Escape
Sleep 500ms
Type "cdx 1"
Sleep 300ms
Enter
Sleep 1000ms
Type "cdx docs"
Sleep 300ms
Enter
Sleep 1200ms

# ---- 3. Filter as you type ---------------------------------------------------
Ctrl+L
Type "# filter as you type"
Sleep 500ms
Enter
Type "cdx"
Sleep 500ms
Enter
Sleep 800ms
Left
Sleep 600ms
Type "0"
Sleep 400ms
Type@200ms "we"
Sleep 900ms
Enter
Sleep 500ms
Space
Sleep 1200ms

# ---- 4. And it is still just cd ----------------------------------------------
Ctrl+L
Type "# and it is still just cd"
Sleep 500ms
Enter
Type "cdx .."
Sleep 200ms
Enter
Sleep 700ms
Type "cdx -"
Sleep 200ms
Enter
Sleep 700ms
Type "cdx ~"
Sleep 200ms
Enter
Sleep 2s
