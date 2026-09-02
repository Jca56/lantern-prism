//! Stable widget identity: a hash of the labels (and indices) on the path
//! from the region root to the widget. Same path next frame, same id.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

impl WidgetId {
    pub const ROOT: WidgetId = WidgetId(FNV_OFFSET);

    #[inline]
    fn mix(mut h: u64, bytes: &[u8]) -> u64 {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        h
    }

    /// Child id from a label.
    pub fn with(self, label: &str) -> WidgetId {
        WidgetId(Self::mix(Self::mix(self.0, label.as_bytes()), &[0xff]))
    }

    /// Child id from an index (list items, repeated widgets).
    pub fn with_index(self, i: usize) -> WidgetId {
        WidgetId(Self::mix(self.0, &(i as u64).to_le_bytes()))
    }

    /// Child id from an arbitrary number (area ids, field ids).
    pub fn with_u64(self, v: u64) -> WidgetId {
        WidgetId(Self::mix(Self::mix(self.0, &v.to_le_bytes()), &[0xfe]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_distinct() {
        let a = WidgetId::ROOT.with("panel").with("ok");
        assert_eq!(a, WidgetId::ROOT.with("panel").with("ok"));
        assert_ne!(a, WidgetId::ROOT.with("panel").with("cancel"));
        assert_ne!(a, WidgetId::ROOT.with("ok"));
        assert_ne!(WidgetId::ROOT.with_index(1), WidgetId::ROOT.with_index(2));
        assert_ne!(WidgetId::ROOT.with("a").with("b"), WidgetId::ROOT.with("ab"));
        assert_ne!(WidgetId::ROOT.with_index(7), WidgetId::ROOT.with_u64(7));
    }
}
