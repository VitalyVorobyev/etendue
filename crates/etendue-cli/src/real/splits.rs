//! Deterministic splits of a dataset's views into two halves (P20c).

use serde::{Deserialize, Serialize};

/// A partition of a dataset's views (capture indices) into two halves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Split {
    /// `interleaved`, `halves`, or `random <seed>`.
    pub name: String,
    pub a: Vec<usize>,
    pub b: Vec<usize>,
}

/// The splits of `n` views, every one deterministic:
/// - **interleaved:** the even views against the odd;
/// - **halves:** the first ⌈n/2⌉ against the rest, in capture order (the poses a robot took first
///   against those it took last);
/// - **random:** `random` seeded shuffles (seeds `seed`, `seed + 1`, …), the first ⌈n/2⌉ of each
///   against the rest.
#[must_use]
pub fn splits(n: usize, random: usize, seed: u64) -> Vec<Split> {
    let half = n.div_ceil(2);
    let split = |name: String, order: &[usize]| {
        let (mut a, mut b) = (order[..half].to_vec(), order[half..].to_vec());
        a.sort_unstable();
        b.sort_unstable();
        Split { name, a, b }
    };
    let interleaved: Vec<usize> = (0..n).step_by(2).chain((1..n).step_by(2)).collect();
    let in_order: Vec<usize> = (0..n).collect();
    let mut out = vec![
        split("interleaved".into(), &interleaved),
        split("halves".into(), &in_order),
    ];
    for k in 0..random as u64 {
        let s = seed.wrapping_add(k);
        out.push(split(format!("random {s}"), &shuffled(n, s)));
    }
    out
}

/// `0..n` shuffled by Fisher–Yates on SplitMix64 from `seed`: the same on every platform.
fn shuffled(n: usize, seed: u64) -> Vec<usize> {
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_split_partitions_the_views_into_halves() {
        let s = splits(7, 3, 10);
        assert_eq!(s.len(), 5);
        assert_eq!(s[0].a, vec![0, 2, 4, 6]);
        assert_eq!(s[0].b, vec![1, 3, 5]);
        assert_eq!(s[1].a, vec![0, 1, 2, 3]);
        assert_eq!(s[4].name, "random 12");
        for split in &s {
            let mut all: Vec<usize> = split.a.iter().chain(&split.b).copied().collect();
            all.sort_unstable();
            assert_eq!(all, (0..7).collect::<Vec<_>>(), "{}", split.name);
            assert_eq!(split.a.len(), 4);
        }
        // Deterministic, and the seeds differ.
        assert_eq!(splits(7, 3, 10), s);
        assert_ne!(s[2].a, s[3].a);
    }
}
