//! Rust halves of the change-detection candidates, over a column shaped like a
//! Bevy 0.20 table column: `n` rows of a 32-byte component plus a parallel `u32`
//! changed-tick array. Benchmarked in `benches/post_run.rs`.

pub const STRIDE: usize = 8; // u32 slots per row (32 bytes)

pub struct Column {
    pub data: Vec<u32>,
    pub ticks: Vec<u32>,
    pub want: Vec<u8>,
}

impl Column {
    pub fn new(n: usize, frac: f64) -> Self {
        let mut seed: u32 = 0x9e37_79b9;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f64 / 4_294_967_296.0
        };
        let mut data = vec![0u32; n * STRIDE];
        let mut want = vec![0u8; n];
        for i in 0..n {
            let w = (rnd() < frac) as u8;
            want[i] = w;
            data[i * STRIDE] = i as u32;
            data[i * STRIDE + 3] = if w == 1 { 1.5f32.to_bits() } else { 0 };
        }
        Self { data, ticks: vec![0; n], want }
    }

    pub fn n(&self) -> usize {
        self.ticks.len()
    }
}

/// Option A: mark every row of the write column changed after the run.
#[inline(never)]
pub fn mark_all(ticks: &mut [u32], tick: u32) {
    ticks.fill(tick);
}

/// Option B with a separate bitset: the post-run pass turning set bits into ticks.
#[inline(never)]
pub fn bits_to_ticks(bits: &[u64], ticks: &mut [u32], tick: u32) {
    for (wi, &word) in bits.iter().enumerate() {
        let mut word = word;
        while word != 0 {
            let bit = word.trailing_zeros() as usize;
            ticks[(wi << 6) + bit] = tick;
            word &= word - 1;
        }
    }
}

/// Builds the bitset the JS side would have produced: one bit per written row.
pub fn bits_from_want(want: &[u8]) -> Vec<u64> {
    let mut bits = vec![0u64; (want.len() + 63) >> 6];
    for (i, &w) in want.iter().enumerate() {
        if w == 1 {
            bits[i >> 6] |= 1 << (i & 63);
        }
    }
    bits
}

/// Option D, before the run: snapshot the column.
#[inline(never)]
pub fn shadow_copy(data: &[u32], shadow: &mut [u32]) {
    shadow.copy_from_slice(data);
}

/// Option D, after the run: per-row memcmp against the snapshot, tick on a difference.
#[inline(never)]
pub fn shadow_diff(data: &[u32], shadow: &[u32], ticks: &mut [u32], tick: u32) -> usize {
    let mut changed = 0;
    for (i, (row, old)) in data
        .chunks_exact(STRIDE)
        .zip(shadow.chunks_exact(STRIDE))
        .enumerate()
    {
        if row != old {
            ticks[i] = tick;
            changed += 1;
        }
    }
    changed
}

/// Option D at 64-row page granularity: compare a page at once, mark the page.
#[inline(never)]
pub fn shadow_diff_pages(data: &[u32], shadow: &[u32], ticks: &mut [u32], tick: u32) -> usize {
    const PAGE: usize = 64 * STRIDE;
    let mut changed = 0;
    for (p, (page, old)) in data.chunks(PAGE).zip(shadow.chunks(PAGE)).enumerate() {
        if page != old {
            let rows = page.len() / STRIDE;
            ticks[p * 64..p * 64 + rows].fill(tick);
            changed += rows;
        }
    }
    changed
}

/// Option C, the Rust half: apply field-write ops (opcode, row, byte offset, f32 bits).
#[inline(never)]
pub fn apply_cmds(ops: &[u32], data: &mut [u32], ticks: &mut [u32], tick: u32) {
    for op in ops.chunks_exact(4) {
        let row = op[1] as usize;
        let off = (op[2] >> 2) as usize;
        data[row * STRIDE + off] = op[3];
        ticks[row] = tick;
    }
}

/// Builds the ops the JS side would have emitted: three field writes per written row.
pub fn cmds_from_want(want: &[u8]) -> Vec<u32> {
    let mut ops = Vec::new();
    for (i, &w) in want.iter().enumerate() {
        if w == 1 {
            for off in [0u32, 4, 8] {
                ops.extend_from_slice(&[1, i as u32, off, 1.0f32.to_bits()]);
            }
        }
    }
    ops
}

/// What a `Changed<T>` consumer pays whatever the option: a scan of the tick array.
/// Returns the count of rows newer than `last_run` (Bevy's `is_newer_than` with no wrap).
#[inline(never)]
pub fn changed_scan(ticks: &[u32], last_run: u32, this_run: u32) -> usize {
    ticks
        .iter()
        .filter(|&&t| this_run.wrapping_sub(last_run) > this_run.wrapping_sub(t))
        .count()
}

/// Dev-only guard: hash a read column to detect a write (the design's read-column check).
#[inline(never)]
pub fn hash_column(data: &[u32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &w in data {
        h ^= w as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
