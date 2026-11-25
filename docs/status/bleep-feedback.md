# Transient canvas feedback verification

Implemented in `src/editor/bleep.rs`; state lives in the egui context, keyed by stable project and level IIDs. Selection and removal snapshots contain geometry, not a cloned document. Switching level/project discards old feedback.

Source references in `.reference/ldtk/src/electron.renderer/`:

- `display/LevelRender.hx:365–439,785–816`: rectangle padding/truncation, Point radius 16 and rounding, raw rounded layer parallax, animation scale and alpha.
- `tool/SelectionTool.hx:90–145,319–346`: individual selection flashes; group deletion does not add a flash.
- `tool/lt/EntityTool.hx:235–264`: entity removal snapshot; Point removal flashes the owner rectangle and unoffset Point position.
- `data/Level.hx:707–733` and `display/LevelRender.hx:773–783`: error priority, red feedback, inspector suppression, speed 0.4 and extra scale 1.
- `page/Editor.hx:1458`, `ui/CommandPalette.hx:179`, `ui/FieldInstancesForm.hx:551–569` and `tool/PickPoint.hx:47`: follow, delayed search, reference-hover flashes and its persistent magenta circle, successful Point picking.
- `LevelTimeline.hx:326–361,418–452`: red undo at speed 0.75, green redo, transaction bounds and the initial-versus-expanded rectangle convention. `GenericLevelElementGroup.hx:731–780`, `SelectionTool.hx:326–337`, `ResizeTool.hx:165` and `EntityInstanceEditor.hx:190` provide the marking callers.

Current hooks cover individual IntGrid/Tile/Entity/Point selection, direct entity removal, Point removal, automatic entity tag/field errors, reference follow/hover, command search, Point picking and undo/redo. Ordinary feedback scales from 1.6 to 1 and fades over approximately 0.2604 seconds at source FPS 60. Error feedback starts at 2.6 and runs at speed 0.4. Layer parallax deliberately follows the source raw offset behavior rather than changing it to the editor coordinate transform.

Reference hover flashes magenta twice on entry, keeps a magenta circle while hovered and removes it on leave. Following invokes the source's two separate feedback calls (follow plus individual selection). Command search instead fits the target level and emits five flashes after 0.2 seconds without selecting the entity or opening its inspector. The initial delayed graphic remains visible at scale 1/alpha 1, as in the original, and repeated pulses have the source 0.03-second transparent interval.

The persistent hover belongs to the widget, not merely the entity IID. The input regression renders two different references to the same entity and verifies that the inactive reference cannot clear the active reference's circle.

History stores only four bounds coordinates in its existing pending snapshot/action; it does not clone a level to discover highlight bounds. Paint, field/Point edits, chain changes, entity creation/removal/transform, group cut/delete/move and paste mark their affected geometry. Markers use raw offsets and rounded entity pivots. Empty/cancelled transactions cannot leak bounds into the next action. A group move marks its destination before the core drops out-of-bounds elements, so undo still highlights a removed entity's destination. Source group deletion adds no separate selection/removal flash.

The error scanner includes hidden layers and offscreen entities in the current level. It selects the first erroneous entity within each layer, with the last erroneous layer replacing earlier candidates, as in the source's nested-loop break behavior. At completion it finds the stable layer UID/entity IID again and revalidates before drawing. The currently inspected entity is suppressed.

Performance tradeoff: the source uses an event-invalidated cached error; this implementation scans incrementally with a soft 1 ms budget checked after each entity. It schedules another scan 0.7 seconds after completion. Large levels can therefore show an error later by the scan's additional frames. One entity's diagnostic evaluation (including a file/reference lookup) and final candidate revalidation can exceed the soft budget. It never scans all project levels periodically, although existing field-reference diagnostics may look up a target elsewhere in the project.

Validation after the final hooks: `scripts/cargo test --bin level-editor bleep::tests --locked --offline` passed 14 tests, `field_tools::tests` passed 7, and core `--test history` passed 20 with one separately designated performance fixture ignored. Tests inspect actual egui shapes, timed delay/repeat/alpha/geometry, actual reference-widget hover entry/leave, undo/redo color and speed, removed group destinations, transaction isolation, deleted entity snapshots, Point erase, level identity reset, hidden/offscreen errors, layer priority, inspector suppression and resumable zero-budget scanning. The offscreen fixture explicitly enlarges the level and asserts successful entity creation. The reference-hover input regression exposed a nested egui context-lock call; moving the frame-counter read outside `data_mut` fixed the observed deadlock panic.

These tests establish the listed source-shaped callers and measured timing, not synchronized frame-by-frame pixel equality between the original process and this process. See the final integrated verification record for whole-workspace results.
