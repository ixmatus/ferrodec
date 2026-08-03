//! Counter mode deterministic sampling stream.
//!
//! Sample `i` of a shard is a pure function of
//! `(campaign, func, stratum, shard, i)`: no sequential generator
//! state exists, so a checkpoint is one integer, resume is O(1), and
//! re-running any index range reproduces its samples byte for byte
//! (the S1 aggregation idempotence argument). The mix is `SplitMix64`,
//! whose full 64 bit avalanche makes per index streams statistically
//! independent for sampling purposes; nothing here is
//! cryptographic and nothing needs to be.

/// One shard's keyed stream.
#[derive(Clone, Copy, Debug)]
pub struct StreamKey(pub u64);

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

fn splitmix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl StreamKey {
    /// Derive the key from the campaign identity fields (FNV-1a over
    /// the label bytes, then one mix round).
    #[must_use]
    pub fn derive(campaign: &str, func: &str, stratum: &str, shard: u32) -> Self {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        for b in campaign
            .bytes()
            .chain([0u8])
            .chain(func.bytes())
            .chain([0u8])
            .chain(stratum.bytes())
        {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01B3);
        }
        Self(splitmix(h ^ (u64::from(shard).wrapping_mul(GOLDEN))))
    }

    /// The draw stream for sample `i`.
    #[must_use]
    pub fn draws(self, i: u64) -> Draws {
        Draws {
            state: splitmix(self.0 ^ i.wrapping_mul(GOLDEN)),
            ctr: 0,
        }
    }
}

/// A per sample sequence of u64 draws (counter mode below the counter
/// mode: each draw is `splitmix(state + n·GOLDEN)`).
#[derive(Clone, Copy, Debug)]
pub struct Draws {
    state: u64,
    ctr: u64,
}

impl Draws {
    pub fn next_u64(&mut self) -> u64 {
        self.ctr = self.ctr.wrapping_add(1);
        splitmix(self.state.wrapping_add(self.ctr.wrapping_mul(GOLDEN)))
    }

    /// Uniform in `[0, bound)` by rejection free scaling (128 bit
    /// multiply high); bias is < 2^-64 which is irrelevant for
    /// sampling.
    pub fn below(&mut self, bound: u64) -> u64 {
        ((u128::from(self.next_u64()) * u128::from(bound)) >> 64) as u64
    }

    /// A uniform decimal coefficient with exactly `digits` digits
    /// (leading digit nonzero), as its decimal string.
    pub fn coefficient(&mut self, digits: u32) -> String {
        let mut s = String::with_capacity(digits as usize);
        s.push(char::from(b'1' + u8::try_from(self.below(9)).unwrap()));
        for _ in 1..digits {
            s.push(char::from(b'0' + u8::try_from(self.below(10)).unwrap()));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_mode_is_stateless() {
        let k = StreamKey::derive("s1", "sin", "decades", 3);
        let a: Vec<u64> = (0..4).map(|_| k.draws(41).next_u64()).collect();
        assert!(a.windows(2).all(|w| w[0] == w[1]));
        let mut d = k.draws(41);
        let first = d.next_u64();
        assert_eq!(first, a[0]);
        assert_ne!(d.next_u64(), first);
    }

    #[test]
    fn shards_and_indices_decorrelate() {
        let k0 = StreamKey::derive("s1", "sin", "decades", 0);
        let k1 = StreamKey::derive("s1", "sin", "decades", 1);
        assert_ne!(k0.draws(0).next_u64(), k1.draws(0).next_u64());
        assert_ne!(k0.draws(0).next_u64(), k0.draws(1).next_u64());
        let kf = StreamKey::derive("s1", "cos", "decades", 0);
        assert_ne!(k0.draws(0).next_u64(), kf.draws(0).next_u64());
    }

    #[test]
    fn coefficient_shape() {
        let mut d = StreamKey::derive("s1", "exp", "edge", 0).draws(7);
        let c = d.coefficient(34);
        assert_eq!(c.len(), 34);
        assert_ne!(c.as_bytes()[0], b'0');
        assert!(c.bytes().all(|b| b.is_ascii_digit()));
    }
}
