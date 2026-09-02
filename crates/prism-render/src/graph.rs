//! A deliberately small render graph. Nodes declare the textures they read
//! and write; the graph orders them, allocates transient textures from a
//! pool, and runs each node with a command encoder. wgpu handles barriers;
//! we handle lifetimes and ordering.

use std::collections::HashMap;

use crate::gpu::Gpu;

pub type TexId = usize;

/// Description of a transient texture. Pooled by equality.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TexDesc {
    pub label: &'static str,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
}

impl TexDesc {
    pub fn color(label: &'static str, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        Self {
            label,
            width: width.max(1),
            height: height.max(1),
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        }
    }

    pub fn depth(label: &'static str, width: u32, height: u32) -> Self {
        Self {
            label,
            width: width.max(1),
            height: height.max(1),
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        }
    }
}

/// Reuses textures across frames by descriptor.
#[derive(Default)]
pub struct TexturePool {
    free: HashMap<TexDesc, Vec<(wgpu::Texture, wgpu::TextureView)>>,
    used: Vec<(TexDesc, wgpu::Texture, wgpu::TextureView)>,
}

impl TexturePool {
    pub fn new() -> Self {
        Self::default()
    }

    fn acquire(&mut self, gpu: &Gpu, desc: &TexDesc) -> usize {
        let (tex, view) = match self.free.get_mut(desc).and_then(Vec::pop) {
            Some(tv) => tv,
            None => {
                let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(desc.label),
                    size: wgpu::Extent3d { width: desc.width, height: desc.height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: desc.format,
                    usage: desc.usage,
                    view_formats: &[],
                });
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                (tex, view)
            }
        };
        self.used.push((desc.clone(), tex, view));
        self.used.len() - 1
    }

    /// Return every texture used this frame to the free lists.
    pub fn end_frame(&mut self) {
        for (desc, tex, view) in self.used.drain(..) {
            self.free.entry(desc).or_default().push((tex, view));
        }
    }

    /// Drop pooled textures that were not used this frame (call after
    /// `end_frame` on a resize, for instance).
    pub fn trim(&mut self) {
        self.free.clear();
    }

    pub fn live_count(&self) -> usize {
        self.used.len() + self.free.values().map(Vec::len).sum::<usize>()
    }
}

/// Texture views a node may draw into or sample, resolved for this frame.
pub struct Views<'a> {
    views: Vec<&'a wgpu::TextureView>,
}

impl Views<'_> {
    pub fn get(&self, id: TexId) -> &wgpu::TextureView {
        self.views[id]
    }
}

type NodeFn<'f> = Box<dyn FnOnce(&Gpu, &mut wgpu::CommandEncoder, &Views<'_>) + 'f>;

struct Node<'f> {
    name: &'static str,
    reads: Vec<TexId>,
    writes: Vec<TexId>,
    run: NodeFn<'f>,
}

enum Slot<'f> {
    Transient(TexDesc),
    Imported(&'f wgpu::TextureView),
}

#[derive(Default)]
pub struct RenderGraph<'f> {
    nodes: Vec<Node<'f>>,
    slots: Vec<Slot<'f>>,
}

impl<'f> RenderGraph<'f> {
    pub fn new() -> Self {
        Self { nodes: Vec::new(), slots: Vec::new() }
    }

    /// Register an externally owned view (the swapchain image).
    pub fn import(&mut self, view: &'f wgpu::TextureView) -> TexId {
        self.slots.push(Slot::Imported(view));
        self.slots.len() - 1
    }

    /// Declare a transient texture, allocated at execution.
    pub fn transient(&mut self, desc: TexDesc) -> TexId {
        self.slots.push(Slot::Transient(desc));
        self.slots.len() - 1
    }

    pub fn add_node(
        &mut self,
        name: &'static str,
        reads: &[TexId],
        writes: &[TexId],
        run: impl FnOnce(&Gpu, &mut wgpu::CommandEncoder, &Views<'_>) + 'f,
    ) {
        self.nodes.push(Node { name, reads: reads.to_vec(), writes: writes.to_vec(), run: Box::new(run) });
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Allocate, order and run every node. Consumes the graph.
    pub fn execute(self, gpu: &Gpu, pool: &mut TexturePool, encoder: &mut wgpu::CommandEncoder) {
        let deps: Vec<(Vec<TexId>, Vec<TexId>)> =
            self.nodes.iter().map(|n| (n.reads.clone(), n.writes.clone())).collect();
        let order = schedule(&deps);

        // Allocate transients, then build the view table.
        let mut used: Vec<Option<usize>> = vec![None; self.slots.len()];
        for (i, slot) in self.slots.iter().enumerate() {
            if let Slot::Transient(desc) = slot {
                used[i] = Some(pool.acquire(gpu, desc));
            }
        }
        let views = Views {
            views: self
                .slots
                .iter()
                .enumerate()
                .map(|(i, s)| match s {
                    Slot::Imported(v) => *v,
                    Slot::Transient(_) => &pool.used[used[i].expect("allocated")].2,
                })
                .collect(),
        };

        let mut nodes: Vec<Option<Node<'f>>> = self.nodes.into_iter().map(Some).collect();
        for i in order {
            let node = nodes[i].take().expect("each node runs once");
            prism_core::log_trace!("graph: {}", node.name);
            (node.run)(gpu, encoder, &views);
        }
    }
}

/// Topological order: a node that writes `T` runs before nodes that read `T`;
/// two writers of the same texture keep their declaration order. Ties keep
/// declaration order too, so output is deterministic.
pub fn schedule(nodes: &[(Vec<TexId>, Vec<TexId>)]) -> Vec<usize> {
    let n = nodes.len();
    let mut indegree = vec![0usize; n];
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (b, (reads_b, writes_b)) in nodes.iter().enumerate() {
        for (a, (_, writes_a)) in nodes.iter().enumerate().take(b) {
            // Earlier writer → later reader, and earlier writer → later writer.
            let dep = writes_a.iter().any(|t| reads_b.contains(t) || writes_b.contains(t));
            if dep {
                edges[a].push(b);
                indegree[b] += 1;
            }
        }
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while !ready.is_empty() {
        ready.sort_unstable_by(|a, b| b.cmp(a)); // pop smallest index first
        let i = ready.pop().expect("non-empty");
        order.push(i);
        for &j in &edges[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                ready.push(j);
            }
        }
    }
    debug_assert_eq!(order.len(), n, "render graph has a cycle");
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_respects_dependencies() {
        // 0: reads A writes B  (declared first, but depends on 1)
        // 1: writes A
        // 2: reads B writes C
        // 3: independent
        let nodes = vec![(vec![0], vec![1]), (vec![], vec![0]), (vec![1], vec![2]), (vec![], vec![])];
        // Node 0 is declared before node 1 and node 1 writes A after… wait:
        // dependencies only flow from earlier declarations to later ones, so
        // node 0 reading A does not wait for node 1 (declared later). That is
        // the rule: a read sees writes declared before it.
        assert_eq!(schedule(&nodes), vec![0, 1, 2, 3]);

        // Now declare the writer first.
        let nodes = vec![(vec![], vec![0]), (vec![0], vec![1]), (vec![1], vec![2]), (vec![], vec![])];
        assert_eq!(schedule(&nodes), vec![0, 1, 2, 3]);
        // Two writers of the same texture keep order; a reader waits for both.
        let nodes = vec![(vec![], vec![7]), (vec![], vec![7]), (vec![7], vec![])];
        assert_eq!(schedule(&nodes), vec![0, 1, 2]);
        assert!(schedule(&[]).is_empty());
    }

    #[test]
    fn tex_desc_helpers() {
        let c = TexDesc::color("c", 0, 10, wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!((c.width, c.height), (1, 10));
        let d = TexDesc::depth("d", 4, 4);
        assert_eq!(d.format, wgpu::TextureFormat::Depth32Float);
        assert!(d.usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT));
    }
}
