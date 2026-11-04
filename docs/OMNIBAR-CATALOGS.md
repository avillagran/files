# Omnibar catalogs

Ctrl+L selects the path field. Enter with `>query` opens the existing command
catalog filtered by query; Enter with `:query` opens existing favorites/places
filtered by query. Bare `>`/`:` opens the corresponding full catalog. The path
commit button uses the same handler. Choose a catalog item explicitly afterward;
opening a catalog never executes its first match or navigates automatically.

Only the path editor interprets these prefixes. The local filename filter and
search content queries stay literal under their existing syntax. A real relative
path starting with either character remains available as `./>name` or `./:name`;
absolute paths work normally. No shell evaluation, URL decoding or path parsing
of the suffix. This is a catalog query, not an executable command line.

The trimmed query is limited to256 UTF8 bytes; any control in the untrimmed suffix
rejects. Preflight precedes cloning or leaving the editor. Invalid input preserves
the draft and current directory, letting the user correct it. Successful dispatch
ends path editing, invalidates its asynchronous paste ticket and reuses each
catalog's current owner/selection/context validation, cursor and AuxIo behavior.
Catalog errors, readonly favorites, disabled commands and explicit execution keep
their existing semantics. No new worker, dependency, index or filesystem probe.

F1 and the bottom-bar tips advertise this entry point. Native key/mouse delivery
is separate from private-handler/framebuffer acceptance. Resource/startup freeze
and new-preview pause remain; no global/resource PASS is inferred.

Validation:823Rust252/79/492+3ignored, Clippy/release/fmt PASS. Scene1793430313608194102 PASS5FB1000/440, all5 inspected; fixture place list/production catalog handlers, root0ACK/sourcehash unchanged. No native input/resource/lifetime acceptance.
