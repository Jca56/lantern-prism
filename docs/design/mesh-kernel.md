# Design: Mesh Kernel (`prism-mesh`)

The heart of Prism. Radial-edge topology (BMesh lineage) stored as flat,
persistent attribute arrays. Read `ARCHITECTURE.md` §8 first.

---

## 1. Plain English

A mesh is points (**vertices**), lines between points (**edges**), and
polygons (**faces**). A **loop** is one corner of one face: "face 7, at vertex
12, arriving along edge 30." Loops exist because some data belongs to a corner,
not a vertex — a vertex shared by six faces can have six different UVs and six
different shading normals.

The expensive question in modeling is *adjacency*: "which faces touch this
edge?" "which edges leave this vertex?" Searching for those on every click
would be hopeless on a million-vertex mesh, so Prism stores the answers as
**rings** (cycles) that are kept correct by every edit:

- Around a **vertex**: a ring of all its edges — the **disk cycle**.
- Around an **edge**: a ring of all faces using it (via their loops) — the
  **radial cycle**. Two faces = a normal edge; one = a boundary; zero = a wire
  edge; three or more = non-manifold. All legal.
- Around a **face**: a ring of its corners in order — the **loop cycle**.

"Stored as flat arrays" means the rings are linked lists whose links are *seat
numbers* (indices into a table) rather than memory pointers. That is what lets
the whole mesh be copy-on-write and undoable with the same `ChunkedVec`
machinery as everything else, and it's what keeps Rust happy: no self-
referential pointer soup, just tables.

---

## 2. Element tables

All fields are `ChunkedVec` columns in an `Arena` per domain. Handles are
typed: `VertH`, `EdgeH`, `FaceH`, `LoopH` (each `Handle<T>` with a generation).

```
Vert   edge: Option<EdgeH>          any one edge in this vertex's disk cycle
       + attributes: position: Vec3, select: bool, hide: bool, …

Edge   v: [VertH; 2]                endpoints, v[0] != v[1]
       disk: [DiskLink; 2]          disk-cycle links, one per endpoint:
                                    DiskLink { next: EdgeH, prev: EdgeH }
       loop: Option<LoopH>          any one loop in this edge's radial cycle
       + attributes: select, hide, seam, sharp, crease: f64, bevel_weight: f64, …

Loop   vert: VertH                  the corner's vertex
       edge: EdgeH                  edge from this corner to the next
       face: FaceH
       next, prev: LoopH            loop cycle (around the face)
       radial_next, radial_prev: LoopH   radial cycle (around the edge)
       + attributes: uv: Vec2 (per layer), color, normal (custom split), …

Face   loop: LoopH                  first loop of the loop cycle
       len: u32                     number of corners (≥ 3)
       + attributes: select, hide, smooth: bool, material_index: u32, normal: Vec3, …
```

The disk cycle needs *two* link pairs on each edge because an edge sits in two
disk cycles (one per endpoint). `disk_side(e, v) -> 0 | 1` picks the pair for
vertex `v`. Walking a vertex's edges:

```rust
fn edges_of(&self, v: VertH) -> impl Iterator<Item = EdgeH> {
    let first = self.vert_edge(v)?;
    let mut e = first;
    loop { yield e; e = self.disk_next(e, v); if e == first { break } }
}
```

Iterators of this shape are provided for every ring: `edges_of(v)`,
`faces_of(v)`, `loops_of_edge(e)`, `faces_of_edge(e)`, `loops_of_face(f)`,
`verts_of_face(f)`, `edges_of_face(f)`, `other_vert(e, v)`, `edge_between(v1, v2)`.

### Storage invariant: topology is attributes
The columns above are ordinary attribute layers with reserved names
(`.edge`, `.v`, `.disk`, `.loop`, `.next`, …) and typed fast-path accessors.
There is exactly one storage mechanism in the mesh. A bool `select` layer and
the `radial_next` layer are the same kind of thing.

---

## 3. Attributes

```rust
pub struct AttributeSet { layers: Vec<Attribute> }          // one per domain
pub struct Attribute { name: Name, kind: AttrKind, data: AttrData, flags: AttrFlags }
pub enum AttrData { Bool(ChunkedVec<bool>), F64(ChunkedVec<f64>), I32(..), U32(..),
                    Vec2(..), Vec3(..), Vec4(..), Color(..), Handle(..), Str(..) }
```

- `Name` is an interned string. Built-in names are constants.
- Adding/removing an element pushes/frees the same slot index in *every* layer
  of that domain, so layers never disagree about length.
- Flags: `INTERNAL` (topology, never shown), `REQUIRED` (position),
  `TEMPORARY` (not saved), `INTERPOLATE` (blend on subdivision/split — UVs
  yes, selection no).
- Attribute *interpolation* is a first-class operation: `split_edge_make_vert`
  averages interpolable point attributes onto the new vertex and splits loop
  attributes correctly on both sides. Tools don't do this by hand.

---

## 4. Euler operators

The only functions allowed to touch topology columns directly. Each is small,
preserves every invariant, and is exhaustively tested. Names follow the
classic literature (BMesh uses the same set); the letters are Split / Join /
Make / Kill × Vert / Edge / Face.

| Operator | Effect |
|---|---|
| `make_vert(pos) -> VertH` | isolated vertex |
| `kill_vert(v)` | vertex must have no edges |
| `make_edge(v1, v2) -> EdgeH` | wire edge; error if one exists |
| `kill_edge(e)` | also kills every face in its radial cycle |
| `make_face(&[VertH]) -> FaceH` | edges must exist and form a closed ring |
| `kill_face(f)` | leaves edges and vertices |
| `split_edge_make_vert(e, v) -> (VertH, EdgeH)` | SEMV: insert a vertex in `e` on `v`'s side; every face using `e` gains a corner |
| `join_edge_kill_vert(e, v) -> EdgeH` | JEKV: inverse of SEMV; `v` must have exactly two edges |
| `split_face_make_edge(f, l1, l2) -> (FaceH, EdgeH)` | SFME: cut `f` between two of its corners |
| `join_face_kill_edge(f1, f2, e) -> FaceH` | JFKE: inverse of SFME; the faces must share only `e` |
| `reverse_face(f)` | flip winding (and the loop cycle direction) |

Everything else — extrude, inset, bevel, loop cut, dissolve, merge, subdivide,
bridge, fill, knife — is composed from these plus attribute writes. This is
what makes "Blender-class" reachable at all: a compound tool cannot corrupt the
mesh because the primitives it uses can't.

### `validate()` — the contract
Runs after every euler op in debug builds and always in tests. Checks:

- every handle stored anywhere is live (generation matches);
- `edge.v[0] != edge.v[1]`; no two edges connect the same vertex pair;
- disk cycles: from every vertex, walking `disk_next` returns to the start,
  every visited edge has that vertex as an endpoint, and `prev` mirrors `next`;
- radial cycles: from every edge with loops, walking `radial_next` returns to
  the start, every loop's `edge` is that edge, and `prev` mirrors `next`;
- loop cycles: from `face.loop`, walking `next` returns after exactly
  `face.len` steps, every loop's `face` is that face, consecutive loops share
  the edge recorded on the first (`loop.edge` connects `loop.vert` and
  `loop.next.vert`), `prev` mirrors `next`;
- `loop.vert` is an endpoint of `loop.edge`;
- attribute layers in a domain all have the same length; free lists don't
  overlap live slots.

Failures report the element, the ring, and the rule, so fuzz failures are
diagnosable from the log alone.

---

## 5. Fuzzing

Own PRNG (PCG32). A fuzz case is `(seed, op_count)`. From a random primitive,
apply random euler ops with random valid arguments chosen *from the current
mesh* (so most ops succeed), validate after each. On failure: print the seed
and the full op trace; the test can be replayed exactly with the seed. Fixed
seed sets run in CI; long runs are manual. Shrinking (dropping ops from the
trace while the failure persists) is a later nicety.

Compound ops get the same treatment one level up: random extrude/dissolve/
merge sequences with validate between.

---

## 6. Deletion, free lists, compaction

Freed slots go on a per-domain free list; the slot's generation is bumped so
stale handles fail `is_live`. Iteration skips dead slots via a `live` bitset
column. A `compact()` op rebuilds all tables densely and returns an old→new
handle remap; it runs only at explicit safe points (never inside an operator)
because it invalidates every handle, including ones in selection history.

---

## 7. Evaluated mesh

```rust
pub struct MeshBuffers {
    pub positions:   Vec<Vec3>,        // f64; cast to f32 camera-relative at upload
    pub normals:     Vec<Vec3>,        // per corner (split normals)
    pub uvs:         Vec<Vec2>,        // per corner, per layer
    pub tri_indices: Vec<u32>,         // 3 per triangle, indexes corners
    pub edge_indices:Vec<u32>,         // 2 per edge for wireframe (incl. wire edges)
    pub tri_to_face: Vec<FaceH>,       // origin maps
    pub corner_to_loop: Vec<LoopH>,
    pub vert_to_vert:  Vec<VertH>,
    pub loose_verts:   Vec<VertH>,
}
```

Built by `evaluate(&Mesh) -> MeshBuffers` in `prism-eval`: triangulate each
face (fast paths for tris/quads, ear clipping for n-gons), compute face
normals (Newell), then corner normals honoring `smooth` and `sharp`, then pack.
Parallel over face chunks. The origin maps are what let the pick pass say
"triangle 88 201" and the operator hear "face 4127."

---

## 8. Numerics

Positions are `f64`. Face normals use Newell's method so non-planar n-gons
don't produce garbage. Anything that decides *topology* from geometry
(intersections, booleans, knife) goes through `prism-geom` predicates, which
start as careful f64 and can become exact without API change.

---

## 9. Non-goals for Phase 1

Subdivision surfaces, booleans, remeshing, sculpt layers, multiresolution,
shape keys, skin weights. Each is an attribute layer or a compound op away and
none of them changes the tables above.
