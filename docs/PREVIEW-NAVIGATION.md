# Moving between items from the preview — 0.0.4

M19 adds previous/next Nerd Font buttons in the header, with the accessible name
“Previous/next item in the folder, Alt+Up/Down”. Both buttons and the shortcuts
call the same Action. They walk the current visible order, including folders
and files without a supported preview; it is not an automatic playlist.

The walk uses indices already present in the model, with no stat/readdir in the UI and no
preloading. Hidden/filter/sort keep the criteria of the source view. At an
end the item is not reopened and no other helper starts; while the
listing is incomplete these controls do not change the selection.

Each real change goes through the existing selection/open_preview: it closes the previous
state, changes generation/signature and cancels; only the last destination stays pending
until ACK/reap. An error on one file keeps the buttons so you can move to another.
Raw Path/OsString names are not lossily converted to open or select;
the visible label may use the path's existing representation.

The controls work on the visible folder; they are not back/forward for Markdown links,
tabs, a playlist or recursive navigation. They do not persist a new index/cache.
488 Rust (141 core/58 helper/289 UI), Clippy workspace/all-targets -Dwarnings,
release and fmt PASS. `preview-neighbor-ui-1793101665648168454.json` functional PASS:
5 framebuffers 1000×650, PNG→WebP→PNG with a raw 0xFF name, without visiting .hidden.png;
selection/raw path/effective FD revision and red/blue/green pixels are checked.
Drawn hit targets ≥32×28 and accessible names checked. The initial edge does not
change the revision or start a decode; a real corrupt-PNG error keeps the controls,
going back recovers the image and back/next/back only publishes the last, blue destination.
Root 0 after callback/ACK, 7 helper identities registered and no longer alive; Z not
accepted. No helper observed alive on close in this scene, and no test of
lifetimes/OS input/clipboard/AT-SPI. The raw and error images were inspected,
with no evident tofu in buttons or header collisions at 1000×650.
Resource/startup freeze in effect: NOT_MEASURED, no budget claimed.

Checkpoint 7ef3852 installed with backup `/home/avillagran/.cache/files-test/installed-backups/1793101735048288545`, release/installation hashes
consistent in `docs/evidence/0.0.4/installed-checkpoint-1793101735171805904.json`. files `3a59002aa2935a715e09a24c29fddd0cf0b267c9ba70e9f07c49eac0d9097340`.
