//! A small, fast, seedable random number generator.
//!
//! Two uses, and they want different things. Scenery — where the cows stand,
//! which lap the tube-men wave on — must come out the same on every visit to a
//! circuit, so it is seeded from the circuit's id. Chaos — which nonsense
//! happens next — must not, so it is seeded from the operating system once.
//! The generator is the same either way: xorshift64*, which is plenty for
//! deciding where a cow goes.

/// xorshift64*, seeded through splitmix64 so that similar seeds (the ids of two
/// neighbouring circuits) do not start out correlated.
#[derive(Clone, Debug)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        // splitmix64 finaliser; also guarantees a non-zero state.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self(z | 1)
    }

    /// Seeded from a name, so a circuit always gets the same scenery.
    pub(crate) fn of(name: &str, salt: u64) -> Self {
        // FNV-1a over the bytes.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in name.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self::new(hash ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// Seeded by the operating system, for the things that should surprise.
    pub(crate) fn random() -> Self {
        let mut bytes = [0u8; 8];
        if getrandom::fill(&mut bytes).is_err() {
            // No entropy source: the clock is surprising enough for cows.
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64);
            return Self::new(now);
        }
        Self::new(u64::from_le_bytes(bytes))
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub(crate) fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[low, high)`.
    pub(crate) fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    /// Uniform in `[-1, 1)`.
    pub(crate) fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    /// A whole number in `0..n`. `n` of zero gives zero.
    pub(crate) fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// True about once in `n`.
    pub(crate) fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    /// A random element.
    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// A unit vector, uniform on the sphere.
    pub(crate) fn direction(&mut self) -> bevy::math::Vec3 {
        let z = self.signed();
        let angle = self.unit() * std::f32::consts::TAU;
        let r = (1.0 - z * z).max(0.0).sqrt();
        bevy::math::Vec3::new(r * angle.cos(), z, r * angle.sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_circuit_always_gets_the_same_dice() {
        let mut a = Rng::of("monza", 1);
        let mut b = Rng::of("monza", 1);
        let mut other = Rng::of("spa-francorchamps", 1);
        let first: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        assert_eq!(first, (0..8).map(|_| b.next_u64()).collect::<Vec<_>>());
        assert_ne!(first, (0..8).map(|_| other.next_u64()).collect::<Vec<_>>());
        assert_ne!(
            Rng::of("monza", 1).next_u64(),
            Rng::of("monza", 2).next_u64()
        );
    }

    #[test]
    fn numbers_stay_inside_their_ranges_and_cover_them() {
        let mut rng = Rng::new(7);
        let (mut low, mut high) = (1.0f32, 0.0f32);
        for _ in 0..20_000 {
            let x = rng.unit();
            assert!((0.0..1.0).contains(&x));
            low = low.min(x);
            high = high.max(x);
            assert!((-3.0..5.0).contains(&rng.range(-3.0, 5.0)));
            assert!(rng.below(7) < 7);
            assert!((rng.direction().length() - 1.0).abs() < 1e-4);
        }
        assert!(low < 0.01 && high > 0.99, "{low} {high}");
        assert_eq!(rng.below(0), 0);
        assert!(Rng::new(0).next_u64() != 0, "a zero seed still runs");
    }
}
