# Prism — Architecture Decision Log

Name: **Prism** (repo: `lantern-prism`, crate prefix `prism-`). Rust + wgpu, everything else ours.
One entry per decision. Status is `Accepted` (Alva signed off) or `Proposed` (Claude's
recommendation, not yet contested). Never delete an entry — supersede it.

---

## D001 — External dependencies: wgpu and winit only
**Status:** Accepted
**Decision:** The only external crates are `wgpu` and `winit`. No `syn`/`quote`
(reflection via `macro_rules!`), no `bytemuck` (own byte casting), no `pollster`
(own `block_on`), no `rand`/`proptest` (own PRNG + fuzz harness), no `log`.
**Why:** Lantern philosophy — fully ours beats better-but-borrowed. A 3D editor
needs nothing we can't write.

## D002 — Persistent copy-on-write document; undo = old roots
**Status:** Accepted
**Decision:** The whole document (scene, objects, meshes, materials, …) is a
persistent data structure. Every container sits behind `Arc`; edits go through
`Arc::make_mut`, cloning only the touched path. Undo history is a list of old
root snapshots. Large arrays are **chunked** (`Vec<Arc<[T; N]>>`) so a single
element write clones one chunk, not the array.
**Why:** No per-operator inverse code, no "forgot to record undo" bugs, free
transactions (panic → document untouched), lock-free background evaluation on
snapshots, trivial autosave, and "adjust last operation" is just re-running the
op on the pre-op snapshot.
**Rejected:** Command pattern with inverses (bit-rots, every op is an undo bug
waiting to happen); full snapshot + diff (O(scene) per step unless you build the
chunking anyway).

## D003 — Radial-edge mesh kernel stored as SoA attribute arrays
**Status:** Accepted
**Decision:** BMesh-style radial-edge topology: verts, edges, faces, loops with
disk cycles on verts and radial cycles on edges. No pointer graph — topology is
stored as structure-of-arrays indexed by **generational handles**
(`loop.next`, `loop.radial_next`, `edge.v0/v1`, `vert.first_edge`, …). Those
arrays are ordinary attribute layers over the four domains (point / edge / face /
corner), so position, selection, UVs, creases and topology all share one
chunked, persistent storage system. Deletion uses free lists + generation
counters; a compaction op remaps handles at safe points.
Two representations, permanently: the **edit mesh** (above) and the
**evaluated mesh** (flat, triangulated, GPU-ready, with origin-index attributes
mapping back to edit elements).
Every kernel op is covered by `validate()` (all invariants) and fuzzed with
random op sequences.
**Why:** N-gons, wire edges, loose verts and non-manifold geometry are
first-class. Half-edge hits a wall on exactly the cases real modeling produces.
**Rejected:** Half-edge (manifold-only); face-vertex indexed (fine for viewers,
awful for interactive topology edits).

## D004 — Precision: f64 on CPU, f32 on GPU with camera-relative rendering
**Status:** Accepted
**Decision:** All CPU-side geometry, transforms and math are `f64`. GPU buffers
are `f32`, produced by subtracting the camera position in `f64` *before*
conversion (camera-relative / "floating origin" rendering).
**Why:** This is a personal, max-capability tool on a 24-core desktop — 2× memory
per coordinate is irrelevant, and f64 removes Blender's famous far-from-origin
jitter entirely. Consumer GPUs run f64 at ~1/64 rate, so the GPU stays f32; the
camera-relative trick means it never sees large numbers.
**Rejected:** f32 everywhere (Blender parity including its warts); generic
scalar trait (unneeded complexity once f64 is universal on CPU).

## D005 — Y-up, right-handed world space
**Status:** Accepted
**Decision:** +Y is up, right-handed. Matches Alva's game editor and glTF; no
axis flip when assets go to the engine.

## D006 — Reflection / property system is the keystone
**Status:** Proposed
**Decision:** `prism-props`: a `macro_rules!`-driven struct definition that
emits the struct *and* its metadata — field names, types, UI hints (hard/soft
range, subtype: distance / angle / color / factor / …), flags (animatable,
hidden), and a generic walk. One description drives: auto property panels,
the adjust-last-operation panel, serialization with stable field IDs, keymap
property editing, animation channels later, and a scripting surface later.
**Why:** Blender's RNA is the reason its UI, undo, animation, keymaps and Python
all agree with each other. Editors without this grow five parallel hand-written
copies of "what fields does this have."

## D007 — Operators are the verb system
**Status:** Proposed
**Decision:** Every user action is an `Operator` with reflected props, an id, a
`poll()` (can it run in this context?), `exec()`, and optional `invoke`/`modal`
for interactive tools (gizmo drags, transform, box select). History stores
`(snapshot_before, op_id, props)`. Menus, hotkeys, gizmos and a searchable
command palette all consume one registry.

## D008 — Job system from day one; UI on main thread
**Status:** Proposed
**Decision:** Own thread pool sized to hardware threads. Jobs are closures over
`Arc<Doc>` snapshots (safe because of D002). Parallel targets: per-object mesh
evaluation, normals, BVH/kd-tree builds, file load/save. Window, input, UI and
wgpu submission stay on the main thread.

## D009 — Viewport is a view, never the truth; GPU ID-buffer picking
**Status:** Proposed
**Decision:** `prism-render` wraps wgpu with a small render graph (named
transient textures, pass ordering, pooled allocation). `prism-viewport` owns
camera, grid, solid/matcap shading, overlays and gizmos. Element/object picking
renders IDs to an `R32Uint` target and reads back a small region; CPU BVH
raycast is added for snapping and placement.

## D010 — Screen → areas → editors → regions
**Status:** Proposed
**Decision:** A screen is tiled into splittable/joinable areas; each hosts an
editor type (3D viewport, outliner, properties, …); editors have regions
(header / main / sidebar). Keymap resolution: region → editor → mode → global.
Layouts are saved in the file. Widget rendering style comes from Alva's existing
renderer (to be shown later — not yet imported).

## D011 — Layered crate architecture; GPU-free below `render`
**Status:** Proposed
```
prism-app       winit loop, wiring
prism-ui        areas/regions, widgets, panels (driven by props)
prism-viewport  camera, overlays, gizmos, picking
prism-render    render graph, wgpu wrap, shaders
prism-text      text renderer (ported later)
prism-eval      evaluated mesh, depsgraph, modifiers (later)
prism-ops       operator registry, history, keymap, modal
prism-doc       datablocks, persistent store, undo, file I/O
prism-mesh      radial-edge kernel, attributes, euler ops, validate
prism-geom      BVH, kd-tree, exact predicates, triangulation, normals
prism-props     reflection / property system
prism-core      chunked persistent arrays, arenas, handles, job system
prism-math      vec / mat / quat / transform (f64)
```
Nothing depends upward. Everything below `prism-render` builds and tests without
a GPU or window. Files stay under 600 lines (flag at 500).

## D012 — Own file format: chunked, versioned, field-ID tagged
**Status:** Proposed
**Decision:** Binary container of typed chunks. Structs serialize with stable
field IDs generated from `prism-props`, so adding fields never breaks old files
and unknown chunks/fields are skipped. Nothing fancier until needed.

## D013 — Phasing: headless foundation before the first window
**Status:** Superseded by D015 (2026-09-01). Original: Accepted (Alva: "most
solid core foundation before a single face ever gets extruded")
0. Workspace, `math`, `core` containers + job system, `props` macro, test harness
1. Mesh kernel + attributes + euler ops + `validate()` + fuzzing — pure CPU
2. Document + undo + operators + keymap — headless, fully tested
3. wgpu: render graph, viewport, grid, matcap, ID picking, first window
4. UI shell: areas, docking, props-driven panels, outliner
5. First tools: select, transform (gizmo-first, see open questions), extrude

## D014 — Name: Prism
**Status:** Accepted
**Decision:** The editor is **Prism**. Crates are `prism-*`; the repo folder is
`lantern-prism` (renamed from the original `lantern-prisim` typo, which was a
strong opening move).

## D015 — Phasing: bedrock, then the UI shell, then the kernel
**Status:** Accepted (Alva, 2026-09-01)
**Supersedes:** D013.
**Decision:** The UI shell is built right after bedrock and before the mesh
kernel. "Foundation before features" is a rule about crate dependencies, not
about phase numbers: `prism-ui` never touches `prism-mesh`, so building the
shell first builds nothing on sand. New order:
0. Bedrock — workspace, `math`, `core` (containers, jobs), `props`, tests.
1. Engine slice — `render` device/surface/2D pass/render graph, `text` port,
   `app` winit loop. Exit: a window with a big rect and big text at 0% idle.
2. Shell — `ui`: areas, split/join/resize, headers, widgets, theme,
   props-driven panels demoed on Preferences. Exit: Alva rearranges the layout.
3. Mesh kernel — old Phase 1, unchanged, headless.
4. Document + operators + undo + keymap — old Phase 2, unchanged, headless.
5. First light — the 3D viewport becomes an editor type inside an area.
6. First extrude.
**Why:** The UI is the part Alva struggles with when there is no framework, so
its design gets locked while nothing depends on it. The engine slice the UI
needs (window, one 2D pass, text, input) is small; the 3D stack is not needed
to draw a panel. The kernel stays headless and can be ground out while the
shell is being tested.

## D016 — UI model: immediate-mode widgets inside a retained area tree
**Status:** Accepted (Alva, 2026-09-01)
**Decision:** The screen → area → region tree is retained and saved in the
file. Inside a region, widgets are re-declared on every redraw (immediate
mode): `ui.slider("UI Scale", &mut p.ui_scale, 1.0..=3.0)`. Per-widget
persistent state (text-field cursor, scroll offset, drag origin) lives in a
small map keyed by a stable widget id. Props-driven auto panels are a walk
over `Reflect::fields()` that emits one widget per field.
**Why:** Blender's model. Least code, easiest to reason about, and auto
panels fall out for free. Redraw-on-demand is unaffected: the UI is only
rebuilt when an event arrives.
**Rejected:** Retained widget tree (Godot Control nodes) — two-way binding
plumbing for every props panel, far more code for the same result.

## D017 — Keyboard focus: click-to-focus, hover-focus as a preference
**Status:** Accepted (Alva, 2026-09-01)
**Decision:** Keyboard events route to the last-clicked area; wheel/scroll
routes to the area under the cursor. The focused area draws a visible border.
A preference switches the keyboard policy to Blender-style hover-focus; the
router reads one enum, so both are one code path.
**Why:** Desktop standard; Alva's mental model is a game editor, not Blender.

## D018 — Text: port `lntrn-text`; `prism-text` is GPU-free; one 2D pass
**Status:** Accepted (Alva, 2026-09-01)
**Decision:** Alva unlocked `~/Projects/Lantern-DE/lntrn-text` (own TTF/CFF
parser, GSUB/GPOS shaping, UAX#9/14/24/29, scanline rasterizer, variable
fonts, COLR/CBDT emoji, PNG decoder, glyph atlas). Its CPU side (`font/`,
`raster/`, `shape/`, `unicode/`, `layout/`) is ported into `prism-text`
nearly verbatim. Its GPU side (`gpu/atlas.rs`, `gpu/pipeline.rs`) is **not**
ported: `prism-text` owns atlas *packing* into a CPU RGBA image with dirty
rects, and `prism-render` uploads that image and draws glyph quads through
the same 2D draw-list pass that draws rects, lines and icons. `bytemuck` is
replaced by `prism-core::bytes`; `lntrn-theme`/`lntrn-draw`/`lntrn-gfx`
usages are replaced by Prism types.
**Layering change to D011:** `prism-text` moves *below* `prism-render` and
builds/tests with no GPU. The order is now
`math → core → props → geom → mesh → doc → ops → eval → text → render →
viewport → ui → app`.
**Why:** One pipeline for all 2D means one vertex stream, one atlas texture,
one clip stack, and the 3D viewport composites into the same frame. Porting a
proven engine beats rewriting 18k lines; keeping it GPU-free keeps it in the
headless test set.

## D019 — Color pipeline: sRGB surface, linear shading, one conversion point
**Status:** Proposed (implemented in Phase 1)
**Decision:** The swapchain uses an sRGB format when available; shaders work
in linear light and the hardware encodes on write. Theme and widget colors are
authored and stored **sRGB-encoded** (`Color::hex(0x141414)` means what the
eye sees) and converted to linear exactly once, when `DrawList` pushes a
vertex. The glyph atlas is `Rgba8Unorm` (linear) holding **premultiplied**
texels: coverage as `(c, c, c, c)`, emoji as premultiplied linear RGBA. The
2D pass blends premultiplied. Dark text on light ground gets lntrn-text's
coverage-gamma thickening in the shader; light text is untouched.
**Why:** One conversion point means no double-encoding bugs; blending in
linear light is correct for shapes; the atlas trick lets one pipeline draw
text and emoji with a single `texel × tint`.

## D020 — Kernel storage: typed topology columns beside attribute layers
**Status:** Proposed (implemented in Phase 3)
**Refines:** D003.
**Decision:** Topology links (`vert.edge`, `edge.v`, `edge.disk`, `loop.next`,
`loop.radial_next`, `face.loop`, …) are **typed `ChunkedVec` columns** on the
four tables, not entries in the named attribute set. User data (position,
selection, UVs, creases, anything a tool adds) lives in the attribute set of
the same table. All columns share one persistent slot allocator per domain
(`Slots`: generation, live bitset, intrusive free list), so both kinds of
column clone in O(chunks), edit copy-on-write, and undo the same way — which
is the property D003 actually cares about.
Three kernel-level operations exist beside the euler set: `weld_verts`
(vertex splice, which the euler operators cannot express), `join_faces`
(replace a region by one n-gon over its boundary loop — the primitive every
dissolve reduces to), and the primitives' `add_face`. Everything else is
composed. `Mesh::paranoid` runs `validate()` after every kernel op; the fuzz
harness and the tests turn it on, debug builds do not (it is O(mesh) per op).
**Why:** Euler operators written against typed columns read like the
literature; against generic layers every link access is a `match`. Speed is
a side benefit. The doc-level guarantee (one storage mechanism, persistent,
undoable) is unchanged.

## D021 — Operators never touch the UI; the UI never touches the document directly
**Status:** Proposed (implemented in Phase 4)
**Decision:** Three rules that make D007 hold in practice.
1. **Input vocabulary lives in `prism-ops::input`** (`Event`, `Key`,
   `Modifiers`, `MouseButton`). Keymaps need it below the UI; the UI
   re-exports it. `prism-app` translates winit into it and nothing above the
   app ever sees winit.
2. **Operators ask, the shell acts.** An operator that needs UI (a menu, the
   palette, a path, quitting) pushes a `UiRequest` on its `Ctx`. The executor
   consumes `Undo`/`Redo`/`HistoryClear` itself and hands the rest to the
   shell, which opens the popup or quits. `wm.call_menu {menu}` is how a
   hotkey opens the Add menu.
3. **Direct property edits are undo steps too.** A `props_panel` edit in the
   Properties editor snapshots the document before the panel runs and, if
   anything changed, pushes a step labelled after the panel (`op_id =
   "ui.edit"`). While the pointer is held, consecutive edits with the same
   label coalesce into one step, so a slider drag is one undo, not sixty.
   Adjust-last-operation is only offered for real operator steps.
The executor is transactional: `exec` runs on the live document after an
O(1) snapshot; on error the snapshot is restored and the error becomes the
status line. Modal operators keep their pre-invoke snapshot until they finish
or cancel. Files save from the document as it is (D002 makes that a snapshot
for free); saving records a history revision so the title bar's dirty mark is
exact.
**Why:** Every door into the room (hotkey, menu, panel, palette, gizmo later)
produces the same kind of history entry and the same rollback behaviour.

## D022 — Viewports draw straight into the swapchain, between clear and UI
**Status:** Proposed (implemented in Phase 5)
**Decision:** A frame is three graph nodes: `clear` (color + one shared
window-sized reverse-Z depth transient), `viewports` (each 3D area sets
viewport + scissor to its body rect and draws grid → solid → wire → vertex
dots into the same swapchain image, clearing depth per area), then `ui`
(the 2D pass with `LoadOp::Load`). The shell skips the panel fill for
viewport bodies so the 3D shows through; everything the UI draws over a
viewport (inner shadow, mode label, popups) lands on top naturally.
Picking is a separate on-demand pass into an `R32Uint` target the size of
the viewport: object ids in object mode, face/edge/vertex ids (kind in the
top two bits) for the edit object, drawn in that order with depth bias so
finer elements win. A 64×64 window around the cursor is read back
**synchronously** (only on a click) and the nearest id of the wanted kind
within the pick radius is chosen. Selection state reaches the GPU as packed
one-byte-per-element flag buffers keyed by `Mesh::selection_version`, so a
click re-uploads flags, never geometry (`Mesh::geometry_version`).
**Why:** No offscreen color textures, no extra composite pass, one 2D draw
call kept. Offscreen rendering arrives when a post effect or material
preview needs it, as a graph change, not an architecture change.
**Rejected:** Rendering viewports to textures and sampling them from the 2D
pass (needs per-command texture binding, breaks the single UI draw).

---

## Open questions
- Viewport interaction model: gizmo-first (game-editor native) vs modal hotkeys
  vs both. Both are modal operators under D007, so the foundation is unaffected.
  Navigation shipped in Phase 5: middle-drag orbit, Shift+middle pan, wheel
  zoom, Alt+left orbit; presets and framing in the View menu.
- Widget look / palette for Prism: decided when there are pixels to look at.
  Known taste rules: sizes in multiples of 5, no glows, no help text, big.
- Resolved 2026-09-01: keyboard focus (D017); text renderer source (D018).
