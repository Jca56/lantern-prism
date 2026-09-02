# Prism — Architecture

Prism is a from-scratch 3D modeling editor. Rust, wgpu, winit — and nothing
else. This document is the north star: what the pieces are, how they fit, and
why. Decisions and their rationale live in `DECISIONS.md`; deep-dives for
individual subsystems live in `design/`. Sections marked **Plain English** are
written for humans who would rather test the editor than read about linked
lists.

---

## 1. Principles

1. **Fully ours.** Two external crates. Everything else — math, containers,
   reflection, text, UI, file format, job system — is written here.
2. **Foundation before features.** Nothing gets built on sand — a crate may
   only stand on crates below it. The UI shell ships before the mesh kernel
   because it does not stand on the kernel (D015).
3. **The document is the truth; the viewport is a picture of it.** Rendering
   never stores state the document doesn't already know.
4. **f64 is the truth; f32 is a transmission format.** All CPU geometry is
   double precision. The GPU sees camera-relative single precision.
5. **Everything the user does is an operator.** Menus, hotkeys, gizmos and the
   command palette are four doors into one room.
6. **One description drives everything.** A struct is described once
   (`prism-props`) and that description generates its UI, its file format, its
   undo behavior and — later — its animation channels and scripting surface.
7. **Headless core.** Everything below the renderer builds and tests with no
   GPU and no window. `cargo test` must pass on a machine with no display.
8. **Big text, big targets.** Accessibility is a requirement, not a theme.
   When in doubt, larger.

---

## 2. The 30-second mental model

**Plain English.** There are exactly three kinds of data in Prism:

```
   ┌──────────────┐   evaluate    ┌──────────────┐    draw     ┌──────────────┐
   │   Document   │ ────────────► │  Evaluated   │ ──────────► │   Viewport   │
   │  (the truth) │               │  (derived)   │             │ (a picture)  │
   └──────────────┘               └──────────────┘             └──────────────┘
          ▲                                                           │
          │  operators (extrude, move, select, …)                     │  clicks
          └───────────────────────────────────────────────────────────┘
```

- The **Document** is what gets saved to disk: meshes, objects, materials,
  cameras, your window layout. Operators are the *only* thing that changes it.
- **Evaluated** data is computed *from* the document — triangulated meshes,
  world-space transforms, modifier results. It is never saved and can always be
  thrown away and rebuilt.
- The **Viewport** turns evaluated data into pixels and turns clicks back into
  "you clicked face #4127 of object *Chair*."

Undo is time travel on the Document. Because the document is a persistent
(copy-on-write) structure, every undo step is just an older version of it,
sharing all unchanged memory with the current one.

---

## 3. Workspace layout

```
prism/
  Cargo.toml                 workspace
  crates/
    prism-math/              vectors, matrices, quaternions, transforms (f64)
    prism-core/              handles, arenas, chunked persistent arrays, jobs, prng
    prism-props/             reflection / property system (macro_rules!)
    prism-geom/              BVH, kd-tree, predicates, triangulation, normals
    prism-mesh/              radial-edge kernel, attributes, euler ops, validate
    prism-doc/               datablocks, document, undo, file I/O
    prism-ops/               operator trait, registry, history, keymap, modal
    prism-eval/              evaluated meshes / scene, caching, (later) depsgraph
    prism-text/              fonts, shaping, layout, raster, atlas packing (GPU-free; ported from lntrn-text)
    prism-render/            wgpu wrapper, render graph, 2D draw-list pass, shaders, GPU caches
    prism-viewport/          3D editor: camera, grid, shading, overlays, picking, gizmos
    prism-ui/                input events, screen / areas / regions, widgets, props-driven panels
    prism-app/               winit loop, event translation, wiring, the `prism` binary
  shaders/                   WGSL, with a tiny in-house include preprocessor
  docs/                      this file, DECISIONS.md, design/*.md
```

**Dependency rule:** a crate may depend only on crates listed *above* it in
`DECISIONS.md` D011. `prism-render` is the first crate allowed to touch wgpu.
Files stay under 600 lines (flag at 500); if a module wants to be bigger, it
wants to be two modules.

---

## 4. `prism-math`

All `f64`. Column-major storage (matches WGSL), column vectors, `M * v`.
Right-handed, **+Y up**, radians everywhere internally (the props system
displays degrees).

Types: `Vec2/3/4`, `Mat3`, `Mat4`, `Quat`, `Transform` (translation +
rotation + scale, composed to `Mat4` on demand), `Ray`, `Plane`, `Aabb`,
`Frustum`. Plus the GPU bridge: `Mat4::to_gpu() -> [[f32; 4]; 4]` and
`Vec3::to_gpu()`. No f32 math types exist on the CPU side at all — if you find
yourself wanting one, you are about to lose precision by accident.

Camera conventions: view space looks down **-Z**; clip-space depth is
**reverse-Z with an infinite far plane** (`Depth32Float`, `Greater` compare),
which makes f32 depth precision effectively uniform instead of collapsing near
the far plane. Free win, decided now because retrofitting it touches every
shader.

---

## 5. `prism-core`

The containers everything else is made of.

- **`Handle<T>`** — `{ index: u32, generation: u32 }`, typed by a marker so a
  vertex handle can't be passed where an edge handle is expected. A slot's
  generation bumps on free, so stale handles are detected, not silently reused.
- **`Arena<T>`** — slot storage with a free list and generations; the backing
  store for handles.
- **`ChunkedVec<T>`** — *the* persistent array. `Vec<Arc<Chunk<T>>>` with
  fixed-size chunks (1024 elements). Cloning is O(chunks) pointer copies;
  writing element `i` calls `Arc::make_mut` on one chunk, so an edit that
  touches 50 vertices of a 5M-vertex mesh copies ~50 chunks, not 5M elements.
  Carries a `version: u64` bumped on every mutation, which evaluation uses as a
  cache key. Chunks are also the natural unit for parallel iteration.
- **`Id`** — `u64`, globally unique within a document, never reused, allocated
  monotonically and saved in the file. Datablocks reference each other by `Id`,
  never by pointer or index.
- **Job system** — a thread pool sized to hardware threads, with `scope`
  (structured fork/join), `parallel_for` over ranges/chunks, and a simple task
  queue for fire-and-forget background work. Jobs capture `Arc` snapshots of
  document data, which makes them lock-free by construction (D002). Window,
  input, UI and GPU submission never leave the main thread.
- **Small utilities we refuse to import:** `block_on` for wgpu's async init,
  byte casting for vertex uploads (`as_bytes` on `#[repr(C)]` POD types, one
  careful `unsafe` in one file), an xorshift/PCG PRNG for fuzz testing, and a
  logging macro with levels.

---

## 6. `prism-props` — the keystone

**Plain English.** Every "thing with settings" in Prism — an object, a material,
an operator's options, a light — is described *once*, in a way the program can
read at runtime. From that one description Prism can build the settings panel,
save/load it, undo it, animate it, and (someday) script it. Blender calls this
RNA; it is the reason Blender's UI, Python and animation never disagree about
what a property is. Editors that skip this step end up with five hand-written
copies of the same information that drift apart.

A `macro_rules!` macro (no `syn` — zero deps) that defines a struct *and* emits
its metadata:

```rust
props! {
    /// Settings for the Extrude Region operator.
    pub struct ExtrudeProps {
        #[range(hard = 0.0.., soft = ..10.0), subtype = Distance]
        pub offset: f64 = 0.0,
        #[label = "Along Normals"]
        pub use_normals: bool = true,
    }
}
```

Emits `struct ExtrudeProps`, `impl Default`, and `impl Reflect` exposing
`fn fields() -> &'static [FieldInfo]` (name, label, type tag, default, hard/soft
range, subtype, flags: `ANIMATABLE | HIDDEN | SKIP_SAVE`), plus `get/set` by
field index into a small `Value` enum (`Bool, I64, F64, Vec2/3/4, Str, Enum,
Id, Handle, Color, …`). Nested structs and `Vec<T>` of reflected types are
supported; enums declare their variants with labels.

Consumers: `prism-ui` (auto panels, adjust-last-operation panel), `prism-doc`
(serialization by stable field id), `prism-ops` (props schema, keymap
overrides), `prism-eval` (animation channels, later), scripting (later).

Field ids are assigned explicitly (`#[id = 3]`) once a struct ships in a saved
file; the macro refuses duplicates. Renaming a field is free; changing its type
requires a new id.

---

## 7. `prism-geom`

Pure geometry with no knowledge of meshes or documents.

- **BVH** over triangles/AABBs (SAH build, parallel via the job system),
  ray queries, closest-point, frustum queries. Used for snapping, placement,
  precise selection, and later booleans.
- **kd-tree** over points for merge-by-distance and snapping.
- **Predicates** — `orient2d/3d`, `incircle/insphere`. Start with f64 +
  careful epsilons; the API is shaped so Shewchuk-style adaptive exact
  arithmetic can replace the internals without touching callers, which
  booleans and intersections will eventually demand.
- **Triangulation** — ear clipping for arbitrary n-gons (projected to the
  best-fit plane), fast paths for triangles and convex quads.
- **Normals** — Newell's method for polygon normals (robust for non-planar
  n-gons), angle-weighted vertex normals, split normals honoring sharp edges.
- **Intersections** — ray/tri, ray/plane, segment/segment, tri/tri.

---

## 8. `prism-mesh`

**Plain English.** A mesh is vertices (points), edges (lines between two
points) and faces (flat-ish polygons). The hard part of a modeling tool is not
storing those — it's answering "what's next to what" instantly: which faces use
this edge, which edges leave this vertex, what's the next corner around this
face. Prism stores those answers permanently as *rings* (cycles) so an edit
never has to search. Full detail in `design/mesh-kernel.md`.

Summary of the design:

- Four element domains: **vertex, edge, face, loop** (a loop is one corner of
  one face — where UVs and per-corner normals live).
- **Radial-edge topology**: each vertex knows a ring of its edges (disk cycle),
  each edge knows a ring of the faces using it (radial cycle), each face knows
  its ring of corners (loop cycle). Non-manifold geometry, wire edges, n-gons
  and loose vertices are all legal.
- **No pointers.** Every link is a `Handle` into a `ChunkedVec`. Topology is
  therefore just more attribute arrays, and the whole mesh is persistent and
  undoable with the same machinery as everything else.
- **Attributes**: named, typed layers per domain — `position`, `select`,
  `hide`, `uv`, `color`, `crease`, `bevel_weight`, `material_index`, `smooth`,
  and anything a tool wants to add. Selection is an attribute, so it undoes,
  saves and evaluates for free.
- **Euler operators**: ~10 primitive topology edits that each preserve every
  invariant (`split_edge_make_vert`, `join_face_kill_edge`, …). Extrude,
  bevel, inset, loop cut, dissolve, merge and subdivide are all compositions
  of these. `validate()` runs after every primitive in debug builds and tests.
- **Evaluated mesh** (`MeshBuffers`): flat, triangulated, with corner normals,
  edge index lists for wireframe, and origin maps (`tri → face`,
  `corner → loop`, `vert → vert`) so overlays and picking can point back at
  the editable element.

---

## 9. `prism-doc`

### Datablocks
`Scene`, `Collection`, `Object`, `Mesh`, `Material`, `Camera`, `Light`,
`Image`, `Screen`/`Workspace` (window layouts), `Preferences` (not saved in the
project file). Each has an `Id`, a `name`, and reflected props. Objects refer
to their data by `Id` (`object.data: Id`), so many objects can share one mesh
and "users" of a datablock are derivable, not tracked.

### The document
```rust
pub struct Doc {
    pub ids:       IdAllocator,
    pub scenes:    Store<Scene>,      // Store<T> = Arc<map Id -> Arc<T>>
    pub objects:   Store<Object>,
    pub meshes:    Store<Mesh>,
    ...
}
```
`Doc: Clone` is O(1) — a handful of `Arc` bumps. Mutation goes through
`doc.meshes.get_mut(id)`, which `Arc::make_mut`s the store and the block, and
then whatever `ChunkedVec` chunk is touched. Nothing else copies.

### Undo
```rust
struct UndoStep { before: Doc, label: String, op_id: OpId, props: Box<dyn Reflect> }
```
`History { steps: Vec<UndoStep>, cursor: usize }`. Undo = `doc = steps[cursor-1].before.clone()`.
Redo = re-apply. **Adjust last operation** = restore `before`, re-run with new
props. Budget is enforced by step count first and measured unique-chunk memory
later. Because an operator that panics never commits, the document is
transactional by construction.

### Selection and modes
Mode (object / edit) is stored per object *in the document* — it undoes and
saves. Element selection is the `select` attribute per domain; the active
element and the ordered **selection history** (needed by order-sensitive tools
like bridge/connect) live on the mesh's edit state. Object selection is a flag
on `Object`; the active object lives on the scene.

### File format
Chunked binary container: a header (magic, format version, app version),
then typed chunks — one per datablock plus a string table. Structs serialize
field-by-field with their `prism-props` field id, so old files load in new
builds (missing fields take defaults) and new files load in old builds
(unknown fields are skipped). Large arrays are stored as raw typed blobs.
Saving happens from a snapshot on a worker thread; the UI never blocks.

---

## 10. `prism-ops`

```rust
pub trait Operator: 'static {
    const ID: &'static str;                 // "mesh.extrude_region"
    const FLAGS: OpFlags;                   // REGISTER | UNDO | BLOCKING | …
    type Props: Reflect + Default;
    fn poll(ctx: &Ctx) -> bool;             // may it run here, right now?
    fn exec(ctx: &mut Ctx, props: &Self::Props) -> Result<Outcome>;
    fn invoke(ctx: &mut Ctx, props: &mut Self::Props, ev: &Event) -> Result<Flow> { exec… }
    fn modal(&mut self, ctx: &mut Ctx, props: &mut Self::Props, ev: &Event) -> Flow;
}
```
`Flow` is `Running | Finished | Cancelled | PassThrough`. Modal operators are
explicit state machines (transform, box select, knife, gizmo drags). The
`Ctx` gives an operator the document, the active area/region/viewport, the
cursor, the current mode and the event — never the UI directly.

The **registry** type-erases operators behind `dyn OpVTable`, keyed by ID.
Menus, keymaps, gizmos and the command palette all invoke through it; the
palette is a fuzzy search over registered IDs and labels.

**Keymap**: data, not code. `KeyItem { trigger, op_id, prop_overrides }`
grouped into named `KeyMap`s by context (`Window`, `Screen`, `View3D`,
`Mesh Edit`, `Object Mode`, …). Resolution walks region → editor → mode →
global and fires the first item whose operator `poll`s true. Keymaps are saved
as a file and editable in Preferences. Whether keyboard input goes to the
region under the cursor or the last-clicked region is a UI decision (Alva's
call — see §13).

---

## 11. `prism-eval`

Turns a `Doc` snapshot into an `EvalScene`: per object, a world `Mat4` and an
`Arc<MeshBuffers>`. Cached by `(mesh Id, mesh version)` so untouched meshes
are reused. Runs on the job system, one job per dirty object, and posts results
to the main thread through a channel; the main thread only uploads what
changed.

This is the seed of the dependency graph. When modifiers, constraints and
drivers arrive, `prism-eval` grows nodes with declared inputs/outputs and
incremental re-evaluation — but the contract "pure function of a snapshot,
memoized by version" never changes.

---

## 12. `prism-render`

- **Device / surface** setup, swapchain, frame pacing, resize.
- **Render graph**: passes declare the transient textures they read/write;
  the graph orders them and allocates from a pooled cache by descriptor. wgpu
  handles barriers; we handle lifetimes and ordering. Deliberately small.
- **GPU caches** keyed by `(Id, version)`: vertex/index buffers per evaluated
  mesh, textures per image. Separate vertex streams (positions, normals, uv,
  color) rather than interleaved, so a position-only edit re-uploads one
  stream.
- **Camera-relative upload**: per frame, per object,
  `model_rel = translate(-cam_pos) * world` computed in f64 and cast to f32;
  the view matrix carries no translation. The GPU never sees a large number.
- **Shaders**: WGSL files in `shaders/` with a tiny `#include`-style
  preprocessor and hot-reload in debug builds.
- **2D pass**: one vertex stream of quads — solid rects, rounded rects (SDF
  corners), lines, glyphs, icons — drawn in one or two calls against a single
  RGBA atlas, with a scissor/clip stack. `prism-ui` emits a `DrawList`;
  `prism-text` supplies glyph quads and the atlas image; this pass draws both.
  The 3D viewport renders into its region of the same frame.
- **Pick pass**: renders object/element IDs to an `R32Uint` target on demand
  (not every frame), copies a small window around the cursor to a staging
  buffer, maps it asynchronously, and resolves next frame. Gizmo parts reserve
  their own ID range, so gizmo hit-testing is the same code path.

---

## 13. `prism-viewport`

The 3D editor. Owns its `Camera` (orbit target/distance/rotation or fly mode;
perspective/orthographic; axis-aligned snaps), the infinite grid on the XZ
plane (Y-up), shading modes (Solid with matcap or studio lights, Wireframe,
later Material Preview), overlays (wire-on-shaded, vertex dots, face centers,
normals, selection tint, active highlight) and gizmos (a 3D transform gizmo is
a modal operator with a drawn handle). Multiple viewports each own a camera.

Interaction model — **gizmo-first** (game-editor native) with hotkey-modal
transforms available too — is a Phase 5 UI decision. Both are modal operators
underneath, so nothing here depends on the answer.

---

## 14. `prism-ui`

**Plain English.** The window is tiled into **areas**. Each area shows one
**editor** (3D viewport, outliner, properties, …). Each editor has **regions**
(a header bar, the main body, an optional sidebar). You can split, join and
resize areas, and the layout is saved with the project. Properties panels are
built automatically from `prism-props`, so every setting in the program gets a
UI the moment it exists.

Screen → `LayoutTree` (binary splits) → `Area { editor: EditorKind, regions }`
→ `Region { kind, rect, scroll }`. The tree is retained and saved; the widgets
inside a region are **immediate-mode** — re-declared on every redraw (D016),
with per-widget persistent state in a map keyed by stable widget id.

Input routing (D017): keyboard goes to the last-clicked area, wheel goes to the
area under the cursor, the focused area draws a border; a preference flips the
keyboard policy to hover-focus. Text comes from `prism-text` (D018). The
widget look is decided when there are pixels to look at. Accessibility rule:
text and hit targets are large by default and scale with a single UI-scale
setting.

Open UI decisions for Alva: default layout, gizmo vs modal transforms, overlay
defaults, palette.

---

## 15. `prism-app`

winit event loop. **Redraw on demand**: an idle editor sits at 0% CPU/GPU. One
frame = pump input → route to UI/ops → if the document changed, kick eval jobs
→ drain finished eval results → upload → build render graph → submit → present.
Single window first; multi-window is a layout-tree question, not an
architecture one.

---

## 16. Testing

- **Unit tests** everywhere below `prism-render`; the workspace tests with no
  GPU or display.
- **`validate()` after every euler operator** in debug builds and always in
  tests.
- **Fuzzing**: seeded PRNG, random operator sequences over random meshes,
  validate after every step; failures print the seed and op trace so they
  replay exactly. Shrinking comes later.
- **Golden tests** for evaluated output (triangulation, normals, file
  round-trips) with byte-stable expectations.
- **Performance gates** as tests with generous ceilings: 5M-triangle eval,
  10k-step undo memory, 1M euler ops.
- **GPU and UI testing is manual** and belongs to the Bug Testing department
  (Alva). Never screenshot from the agent side.

---

## 17. Phases and exit criteria

Order per D015: bedrock, then the UI shell, then the kernel.

**Phase 0 — bedrock.** Workspace, `prism-math` (tested against hand-derived
cases), `prism-core` (`ChunkedVec` persistence semantics proven by tests,
job system `parallel_for` and `scope`), `prism-props` macro emitting metadata
for a sample struct with generic serialization round-tripping.
*Done when `cargo test` is green and a reflected struct can be walked, saved,
and loaded without any per-type code.*

**Phase 1 — engine slice.** `prism-render`: device, surface, resize, frame
pacing, the small render graph, the 2D draw-list pass. `prism-text`: port of
lntrn-text's CPU side plus CPU atlas packing. `prism-app`: winit loop,
redraw-on-demand, input events. *Done when a window shows a big rectangle
and big text, resizes cleanly, and idles at 0% CPU/GPU.*

**Phase 2 — the shell.** `prism-ui`: screen/area/region tree, split, join,
drag-resize, header bars, the widget set (button, toggle, slider/number,
text field, dropdown, menu, tabs, scroll), theme + UI scale, focus routing,
props-driven auto panels demoed on Preferences. *Done when Alva can
rearrange the layout, change a preference, and everything is chonky.*

**Phase 3 — mesh kernel.** Four tables, cycles, attributes, all euler
operators, `validate()`, fuzz harness, primitives (plane, cube, UV sphere,
cylinder, grid) built via euler ops only, first compound ops (extrude,
delete, merge, split, dissolve), evaluated mesh with triangulation and
normals. *Done when a 1M-op fuzz run passes on a fixed seed set and goldens
match.*

**Phase 4 — document.** Datablocks, `Doc`, undo/redo, operator trait +
registry, keymap resolution, adjust-last-operation, file save/load, outliner
and object properties in the shell. *Done when 10k undo steps stay inside
budget and a file round-trips byte-for-byte after load → save.*

**Phase 5 — first light.** Viewport editor type inside an area: grid, matcap
solid shading, camera-relative upload, reverse-Z, orbit/pan/zoom, ID picking
for objects and elements. *Done when clicking a vertex of a 5M-triangle mesh
selects it at 60+ fps.*

**Phase 6 — the first extrude.** Selection tools, transform gizmo, hotkey
transforms, extrude. *Done when a face gets extruded and someone yells.*

Beyond: modifiers + depsgraph, materials + material preview, glTF/OBJ import
and export (own JSON parser), UV editing, animation, booleans, sculpt, a
scripting surface over the props/ops systems. All designed-for, none built yet.

---

## 18. Performance targets

Personal machine, maximum ceiling: 60+ fps orbiting 5M triangles with
overlays; undo under one frame; idle at 0% CPU; cold start under one second;
multi-core saturation on eval, BVH build and file I/O.
