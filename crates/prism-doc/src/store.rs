//! `Store<T>`: an ordered map of datablocks behind `Arc`s. Cloning shares
//! everything; `get_mut` copies only the map spine and the one block.

use std::collections::BTreeMap;
use std::sync::Arc;

use prism_core::Id;

#[derive(Debug)]
pub struct Store<T> {
    map: Arc<BTreeMap<Id, Arc<T>>>,
}

impl<T> Clone for Store<T> {
    fn clone(&self) -> Self {
        Self { map: Arc::clone(&self.map) }
    }
}

impl<T> Default for Store<T> {
    fn default() -> Self {
        Self { map: Arc::new(BTreeMap::new()) }
    }
}

impl<T: Clone> Store<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn contains(&self, id: Id) -> bool {
        self.map.contains_key(&id)
    }

    pub fn get(&self, id: Id) -> Option<&T> {
        self.map.get(&id).map(|a| &**a)
    }

    /// Mutable access: un-shares the spine and this block only.
    pub fn get_mut(&mut self, id: Id) -> Option<&mut T> {
        Arc::make_mut(&mut self.map).get_mut(&id).map(Arc::make_mut)
    }

    pub fn insert(&mut self, id: Id, value: T) {
        Arc::make_mut(&mut self.map).insert(id, Arc::new(value));
    }

    pub fn remove(&mut self, id: Id) -> Option<T> {
        Arc::make_mut(&mut self.map).remove(&id).map(|a| Arc::try_unwrap(a).unwrap_or_else(|a| (*a).clone()))
    }

    /// Blocks in id order.
    pub fn iter(&self) -> impl Iterator<Item = (Id, &T)> {
        self.map.iter().map(|(id, a)| (*id, &**a))
    }

    pub fn ids(&self) -> impl Iterator<Item = Id> + '_ {
        self.map.keys().copied()
    }

    /// Does this store share its spine with `other` (nothing changed)?
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.map, &other.map)
    }

    /// Is block `id` the same allocation in both stores?
    pub fn block_ptr_eq(&self, other: &Self, id: Id) -> bool {
        match (self.map.get(&id), other.map.get(&id)) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_on_write() {
        let mut a: Store<String> = Store::new();
        a.insert(Id(1), "one".into());
        a.insert(Id(2), "two".into());
        let b = a.clone();
        assert!(a.ptr_eq(&b));
        a.get_mut(Id(1)).unwrap().push('!');
        assert!(!a.ptr_eq(&b), "spine copied");
        assert!(a.block_ptr_eq(&b, Id(2)), "untouched block still shared");
        assert!(!a.block_ptr_eq(&b, Id(1)));
        assert_eq!(b.get(Id(1)).unwrap(), "one");
        assert_eq!(a.get(Id(1)).unwrap(), "one!");
        assert_eq!(a.remove(Id(2)), Some("two".into()));
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 2);
        assert_eq!(b.ids().collect::<Vec<_>>(), vec![Id(1), Id(2)]);
    }
}
