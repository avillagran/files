# Data models, batching and thread feeds (Unreleased, after 0.0.10)

Apps that show data which changes all the time (a chat client's thread
list, a log, a transfer queue) should never rebuild a view from a whole
snapshot. The toolkit makes the incremental path the easy one:

| Need | Use |
|---|---|
| A collection shown in a list, table or tree | `model::ListModel<K, T>` + `List::from_model` / `Table::from_model` / `Tree::from_model` |
| The protocol sends changes | `ListModel` operations (`push`, `insert`, `remove_key`, `move_key`, `upsert`, `patch`) or `apply(Vec<Diff>)` |
| The protocol still sends whole snapshots | `ListModel::sync_from(snapshot)` (minimal diff by key) |
| Many changes at once | `model.transaction(|m| ..)` or `state::batch(|| ..)`: one notification |
| A background thread produces the data | `model.feed()` (`model::Feed`), `state.feed()` (`StateFeed`), `text.text_feed()` (`TextFeed`) |
| Streaming text | `State<String>::push_str` / `set_text`; consumers use `subscribe_edit` and `Edit::Appended` |
| A plain value | `State<T>`: `set` and `set_if_changed` never notify for an equal value |

## ListModel

```rust
use fltk_omarchy::{list::{List, ListRow}, model::ListModel};

#[derive(Clone, PartialEq)]
struct Thread { id: String, title: String, status: String, unread: u32 }

let threads = ListModel::new(|t: &Thread| t.id.clone());   // the stable key
let list = List::from_model(&threads, |t| ListRow::new(&t.title).detail(&t.status));

threads.push(Thread { id: "t1".into(), title: "Port o3".into(), status: "idle".into(), unread: 0 });
threads.patch(&"t1".to_string(), |t| t.status = "running".into());   // one row redraws
threads.move_key(&"t1".to_string(), 0);                              // bump to the top
threads.transaction(|m| {                                            // one notification
    m.remove_key(&"old".to_string());
    m.upsert(Thread { id: "t2".into(), title: "New".into(), status: "idle".into(), unread: 1 });
});
```

- Items keep the app's key (`K: Clone + Eq + Hash`) and get a stable
  `ItemId` (`u64`) from the model. Views, `KeyboardPanel` cursor and
  selection use the ID, so they survive moves, sorts and snapshots.
  `id_of(&key)`, `key_of(id)`, `index_of(id)`, `with_key(&key, |t| ..)`.
- Inserting a key that exists replaces that item and moves it (no
  duplicate keys).
- `patch` clones the item once to compare and does not notify when nothing
  changed; `patch_with(|t| -> bool)` skips the clone. A patch may change
  the key: the item keeps its ID.
- `sort_by` reports a reset (IDs survive); `retain`, `clear`,
  `remove_range`, `extend`, `count_state()` (a `State<usize>`).

### Changes

Listeners (`model.subscribe(|model, changes| ..)`) get a `Changes`:

| Method | Meaning |
|---|---|
| `ops()` | `Change::Inserted { at, len }`, `Removed { at, len }`, `Moved { from, to }`, in order, each relative to the list right before it |
| `updated()` | IDs of existing items whose value changed (each once) |
| `inserted()`, `removed()` | IDs that appeared / went (drop per-item caches on `removed`) |
| `is_reset()` | the order changed beyond what `ops` lists (sort, a large reorder, more than 256 ops in one notification); IDs are still valid |
| `is_structural()` | anything but in-place updates |
| `map_index(old)` | where the item at `old` is now (None if removed or after a reset) |

### Snapshots: `sync_from`

Protocols that re-send everything do not need to change first:

```rust
// on every snapshot from the server
threads.sync_from(snapshot_threads);   // Vec<Thread> in display order
```

`sync_from` compares by key: items in the common key prefix and suffix
are only compared with `==`; in the window between, it removes what is
gone, keeps the longest increasing subsequence of survivors in place and
moves only the others (bumping one thread to the top is one `Moved`, not
2 000 changes), inserts the new keys, and replaces items that are not `==`.
An unchanged snapshot notifies nobody. More than 64 moves in one snapshot
reorder in one pass and report a reset. The randomized unit test checks
that the model equals the snapshot, the ops replay to it, the moves are
minimal and exactly the changed items are reported.

## Views

`List::from_model(&model, |item| ListRow)`, `Table::from_model(&panel,
&model, columns, |item, column| text)` and `Tree::from_model(&panel,
&model, |item| TreeNode, |item| parent_key)`:

- **Only changed rows redraw.** Each view remembers which ID it drew in
  each visible row. After a change it compares, and damages only the rows
  that now show another item or an updated one (fltk damage areas; rows
  outside the damaged area are not even built). A status patch on a
  visible thread repaints one row; on an offscreen thread, nothing.
- **Scroll anchoring.** When rows are inserted or removed above the
  viewport, the view stays on its first visible item (and a running wheel
  animation continues). At the very top new items show up, as in a chat
  list. The UI test checks the rows are pixel-identical after inserting
  above.
- **Selection by key.** `List` keeps the selected ID (`selected_id()`)
  and updates `bind_selected` with the new index; `Table` and `Tree` use
  the panel bound with `model.bind_panel(&panel)` (cursor, anchor and
  selection by ID; the host redraws only when they lose an item).
- Custom canvases (several `KeyboardPanel` regions in one widget): call
  `model.bind_panel(&panel)` and `model.subscribe(..)` to repaint what
  changed; the binding repaints the host by itself only when the cursor
  or the selection lost an item.
- `Tree::from_model` builds the tree from a flat model and a parent key:
  an update that keeps the parent and the expander redraws one row;
  anything else rebuilds the visible rows (anchored), keeping expanded
  nodes, cursor and selection.
- `invalidations()` on each view counts full redraws, single rows and
  no-op notifications (tests, benchmarks).

Count-based `List::new(&count, row)`, `Table::new` and `Tree::new` are
unchanged.

## Batching

`state::batch(|| ..)` (also `fltk_omarchy::batch`) defers notifications:
values change immediately, and when the outermost batch ends each changed
`State` notifies once with its final value and each `ListModel` once with
all its changes. Derived states (`map`, `combine`) follow at the end.
`model.transaction(|m| ..)` is the same for one model.

Everything posted with `task::post` runs inside one batch per event-loop
iteration, and so do the feeds. A burst of 500 updates between two loop
iterations is one notification and one paint (the UI test pushes 501
diffs from a thread and sees exactly one notification).

fltk already paints once per loop iteration (redraws only mark damage);
with batching, the work before the paint is also done once.

## Thread feeds

The rule stays: never touch a `State` or a model from another thread. The
sanctioned path:

```rust
let feed = threads.feed();                 // on the UI thread; Send + Sync + Clone
std::thread::spawn(move || {
    feed.patch("t1".to_string(), |t| t.unread += 1);
    feed.move_to("t1".to_string(), 0);
    feed.sync(snapshot);                    // a snapshot drops the diffs queued before it
});
```

- A feed queues behind a mutex and wakes the UI thread once until it
  drains (`fltk::app::awake`); the UI applies everything queued in one
  transaction. No timer, no polling: idle stays at 0 % CPU.
- `feed.max_rate(60.0)` (or `set_max_rate`) applies at most 60 times per
  second: a wake sooner than that arms one fltk timeout instead.
- `state.feed()` → `StateFeed<T>`: the latest value wins. `StateFeed`
  and `TextFeed` have `max_rate` too.
- `text.text_feed()` → `TextFeed`: `push_str(chunk)` from the thread;
  chunks pushed between two loop iterations become one append.
- `is_connected()` is false once the State or model is gone (pushes are
  then dropped). A feed's UI-side hook is released on the first push after
  its State or model is dropped.

## Streaming text and the `Edit` hint

For text views (a markdown renderer, a log) that should lay out only what
is new:

```rust
use fltk_omarchy::state::{Edit, State};
let answer = State::new(String::new());
answer.subscribe_edit(|text, edit| match edit {
    Edit::Appended { from } => { /* parse/layout text[from..], keep the rest */ }
    _ => { /* Edit::Replaced: rebuild from text */ }
}).forever();
answer.push_str("Hello");        // in place, no copy of the existing text
answer.set_text("Hello, world"); // a protocol that re-sends the whole text: still an append
```

- `Edit::Appended { from }`: `value[from..]` is new, everything before
  `from` is unchanged (byte offset, always a char boundary, for `String`;
  item index for `Vec`). `Edit::Replaced`: anything may have changed.
  `edit.new_range(len)` gives the new part either way.
- Edits merged in a batch (`Edit::then`): appends stay one append from the
  earliest offset; anything else becomes `Replaced`. Consumers should
  still handle `from` lower than what they have seen (treat it as "re-lay
  out from there").
- `State::modify(|v| -> Option<Edit>)` edits any value in place and
  reports its own hint; `State<Vec<T>>::push` / `extend` report appends.
- Listeners read the value in place (no clone per notification, even for
  a 1 MB string). `watch_edit` also runs once now with `Replaced`.
- The enum is `#[non_exhaustive]`: match `Appended` and treat everything
  else as `Replaced`.

## Cost

Measured with `examples/stream_bench.rs` (2 000 threads, ~1 MB/s of text,
30–100 events/s, native Wayland). At 1200×800 and 100 events/s the UI
thread goes from 18.4 % of a core (naive snapshots) to 11.1 % with
`sync_from`, 10.1 % with a `Feed` and 6.7 % with feeds capped at 60 Hz;
the worst frame from 7.3 ms to 2.6 ms (`sync_from`) and 0.6 ms (capped);
the data handed to the UI thread from 55.9 MB/s to 1 MB/s with feeds.
Idle after the stream: 0.00 % CPU. Tables and method:
[PERFORMANCE.md](PERFORMANCE.md#streaming-updates-unreleased).

## Limitations

- Models are `Rc`-based and live on the UI thread; only feeds cross
  threads.
- A partial redraw rebuilds the rows inside the bounding box of the
  damaged area (FLTK's clip test uses the region's extents).
- Hover follows the screen position, not the item.
- `Tree::from_model` rebuilds the visible rows on structural changes (the
  index from parent to children is O(n) to rebuild).
- No stick-to-bottom mode for chat transcripts yet; the anchoring keeps the
  first visible item.
