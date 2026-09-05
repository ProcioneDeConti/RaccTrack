//! Generic Reed-Solomon over GF(256), parameterized the way avionics FEC
//! specs (and the classic Phil Karn `fec` library dump978 itself vendors)
//! describe it: a primitive polynomial, a first-consecutive-root exponent
//! (`fcr`), a root spacing (`prim`), and a parity-symbol count (`nroots`).
//!
//! UAT's three message classes are each a *shortened* code (fewer than 255
//! total symbols) of one shared GF(256) field. Karn's implementation handles
//! shortening via an explicit `pad` count against a fixed NN=255 buffer —
//! but that's purely an artifact of sharing one fixed-size table/loop across
//! differently-shortened codes, not a mathematical difference. A shortened
//! RS code decodes correctly as an ordinary length-n code with the same
//! generator roots, searching only its own n positions instead of all 255
//! — so this implementation just uses `n = data.len() + nroots` directly
//! and has no notion of `pad` at all. Verified against known-good UAT
//! parameters via round-trip tests below (encode, corrupt up to the
//! correctable limit, decode, compare) rather than against a reference
//! implementation's internals.

pub struct Gf256 {
    exp: [u8; 512],
    log: [i32; 256],
}

impl Gf256 {
    /// `poly` is the primitive polynomial's low 8 bits (e.g. `0x87` for
    /// `x^8+x^7+x^2+x+1`, conventionally written `0x187` with the implicit
    /// leading `x^8` term dropped).
    pub fn new(poly: u16) -> Self {
        let mut exp = [0u8; 512];
        let mut log = [-1i32; 256];
        let mut x: u16 = 1;
        for i in 0..255usize {
            exp[i] = x as u8;
            log[x as usize] = i as i32;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= poly | 0x100;
            }
        }
        for i in 255..512 {
            exp[i] = exp[i - 255];
        }
        Gf256 { exp, log }
    }

    /// `alpha^e` for any integer exponent (negative wraps correctly).
    fn pow(&self, e: i32) -> u8 {
        let m = e.rem_euclid(255) as usize;
        self.exp[m]
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }
        self.pow(self.log[a as usize] + self.log[b as usize])
    }

    fn div(&self, a: u8, b: u8) -> u8 {
        debug_assert!(b != 0);
        if a == 0 {
            return 0;
        }
        self.pow(self.log[a as usize] - self.log[b as usize])
    }

    fn inv(&self, a: u8) -> u8 {
        debug_assert!(a != 0);
        self.pow(-self.log[a as usize])
    }
}

/// A concrete RS code: a field plus (fcr, prim, nroots).
pub struct RsCode {
    gf: Gf256,
    fcr: i32,
    prim: i32,
    nroots: usize,
    /// Generator polynomial, coefficients highest-degree first, length
    /// `nroots+1` (roots are `alpha^(fcr + i*prim)` for `i in 0..nroots`).
    /// Only `encode()` (test-only — this app never transmits UAT) reads
    /// this; production code just decodes real over-the-air codewords.
    #[allow(dead_code)]
    gen: Vec<u8>,
}

impl RsCode {
    pub fn new(gfpoly: u16, fcr: i32, prim: i32, nroots: usize) -> Self {
        let gf = Gf256::new(gfpoly);
        let mut gen = vec![1u8];
        for i in 0..nroots {
            let root = gf.pow(fcr + i as i32 * prim);
            let mut next = vec![0u8; gen.len() + 1];
            // new_g(x) = g(x)*(x + root): the x*g(x) term keeps each
            // coefficient's array index (multiplying by x raises degree by
            // one, which in this highest-degree-first array is a same-index
            // carry, not a shift — index j already means "one degree higher
            // than a length-shorter g would place it"), while root*g(x)
            // lands one index further out.
            for (j, &c) in gen.iter().enumerate() {
                next[j] ^= c;
                next[j + 1] ^= gf.mul(c, root);
            }
            gen = next;
        }
        RsCode { gf, fcr, prim, nroots, gen }
    }

    /// Systematic encode: `nroots` parity bytes for `data`, via LFSR
    /// polynomial division by the generator (standard shift-register form).
    /// Test-only — this app decodes real UAT traffic, never transmits it.
    #[allow(dead_code)]
    pub fn encode(&self, data: &[u8]) -> Vec<u8> {
        let mut parity = vec![0u8; self.nroots];
        for &d in data {
            let feedback = d ^ parity[0];
            for i in 0..self.nroots - 1 {
                parity[i] = parity[i + 1] ^ self.gf.mul(feedback, self.gen[i + 1]);
            }
            parity[self.nroots - 1] = self.gf.mul(feedback, self.gen[self.nroots]);
        }
        parity
    }

    /// Corrects `codeword` (data followed by its `nroots` parity bytes) in
    /// place. Returns the number of corrected symbols, or `None` if the
    /// error pattern exceeds the code's correction capability (`nroots/2`
    /// symbols) — detected, not silently mis-corrected: the found error
    /// count is cross-checked against the error-locator polynomial's degree
    /// (see the Chien-search comment below).
    pub fn decode(&self, codeword: &mut [u8]) -> Option<usize> {
        let n = codeword.len();
        let gf = &self.gf;

        // Syndromes: S[j] = codeword(alpha^(fcr+j*prim)), Horner-evaluated
        // treating codeword[0] as the highest-degree coefficient. This is
        // exactly the same computation whether or not the code is
        // "shortened" — see the module doc.
        let mut synd = vec![0u8; self.nroots];
        let mut any_nonzero = false;
        for (j, s) in synd.iter_mut().enumerate() {
            let root = gf.pow(self.fcr + j as i32 * self.prim);
            let mut acc = 0u8;
            for &b in codeword.iter() {
                acc = gf.mul(acc, root) ^ b;
            }
            *s = acc;
            any_nonzero |= acc != 0;
        }
        if !any_nonzero {
            return Some(0);
        }

        // Berlekamp-Massey over GF(256) (no erasures — hard-decision only,
        // matching what this receiver can actually supply).
        let mut lambda = vec![0u8; self.nroots + 1];
        let mut prev = vec![0u8; self.nroots + 1];
        lambda[0] = 1;
        prev[0] = 1;
        let mut l = 0usize; // current LFSR length
        let mut m = 1i32; // steps since last update
        let mut b = 1u8; // last discrepancy that caused an update
        for r in 0..self.nroots {
            let mut delta = synd[r];
            for i in 1..=l {
                delta ^= gf.mul(lambda[i], synd[r - i]);
            }
            if delta == 0 {
                m += 1;
            } else if 2 * l <= r {
                let t = lambda.clone();
                let coef = gf.div(delta, b);
                for i in 0..prev.len() {
                    if (i as i32) - m >= 0 {
                        let shifted = prev[i - m as usize];
                        if shifted != 0 {
                            lambda[i] ^= gf.mul(coef, shifted);
                        }
                    }
                }
                l = r + 1 - l;
                prev = t;
                b = delta;
                m = 1;
            } else {
                let coef = gf.div(delta, b);
                for i in 0..prev.len() {
                    if (i as i32) - m >= 0 {
                        let shifted = prev[i - m as usize];
                        if shifted != 0 {
                            lambda[i] ^= gf.mul(coef, shifted);
                        }
                    }
                }
                m += 1;
            }
        }

        // Chien search: try every real position 0..n-1. Array index `p`
        // holds the coefficient of x^(n-1-p) in the codeword polynomial, so
        // its "location" is X_p = alpha^(n-1-p); p is an error position iff
        // lambda(X_p^-1) == 0.
        let mut error_pos = Vec::new();
        for p in 0..n {
            let x_inv = gf.pow(-((n as i32 - 1 - p as i32)));
            let mut acc = 0u8;
            let mut pw = 1u8; // x_inv^0
            for &lc in &lambda {
                acc ^= gf.mul(lc, pw);
                pw = gf.mul(pw, x_inv);
            }
            if acc == 0 {
                error_pos.push(p);
            }
        }
        let lambda_degree = lambda.iter().rposition(|&c| c != 0).unwrap_or(0);
        if error_pos.len() != lambda_degree {
            return None; // more errors than this code can correct
        }
        if error_pos.is_empty() {
            // Syndromes were nonzero but no roots found at all — corrupt
            // beyond recognition rather than a clean 0-error case.
            return None;
        }

        // Forney: error-evaluator polynomial Omega(x) = S(x)*Lambda(x) mod
        // x^nroots (only the low `nroots` coefficients matter).
        let mut omega = vec![0u8; self.nroots];
        for i in 0..self.nroots {
            let mut acc = 0u8;
            for j in 0..=i {
                if j < lambda.len() && (i - j) < synd.len() {
                    acc ^= gf.mul(lambda[j], synd[i - j]);
                }
            }
            omega[i] = acc;
        }
        // Lambda'(x) (formal derivative over GF(2^m), char 2: d/dx x^i is
        // i*x^(i-1), and i*c collapses to c if i is odd, 0 if i is even —
        // so Lambda'(x) has a nonzero term at x^k only when k is *even*
        // (k=i-1 for odd i), equal to lambda[k+1]. Keeping the same
        // ascending-index length (rather than packing the surviving
        // coefficients consecutively) matters: packing would silently
        // shift every surviving term to the wrong degree.
        let lambda_prime: Vec<u8> = (0..lambda.len())
            .map(|k| {
                if k % 2 == 0 {
                    lambda.get(k + 1).copied().unwrap_or(0)
                } else {
                    0
                }
            })
            .collect();

        for &p in &error_pos {
            let x = gf.pow(n as i32 - 1 - p as i32);
            let x_inv = gf.inv(x);
            let mut omega_val = 0u8;
            let mut pw = 1u8;
            for &oc in &omega {
                omega_val ^= gf.mul(oc, pw);
                pw = gf.mul(pw, x_inv);
            }
            let mut lp_val = 0u8;
            pw = 1u8;
            for &lc in &lambda_prime {
                lp_val ^= gf.mul(lc, pw);
                pw = gf.mul(pw, x_inv);
            }
            if lp_val == 0 {
                return None;
            }
            // Standard Forney magnitude generalized for a nonzero fcr
            // (char-2 field, so the textbook "-1" sign vanishes — XOR is
            // its own inverse): e = X^(1-fcr) * Omega(X^-1) / Lambda'(X^-1),
            // where X = alpha^(n-1-p) is this position's location value.
            let x_exp = n as i32 - 1 - p as i32;
            let x_pow_1_minus_fcr = gf.pow(x_exp * (1 - self.fcr));
            let mag = gf.div(gf.mul(x_pow_1_minus_fcr, omega_val), lp_val);
            codeword[p] ^= mag;
        }

        Some(error_pos.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three real UAT codes (see `ingest::uat::mod` for where these
    /// numbers come from: dump978's `fec.c` `init_rs_char` calls).
    fn short_code() -> RsCode {
        RsCode::new(0x87, 120, 1, 12)
    }
    fn long_code() -> RsCode {
        RsCode::new(0x87, 120, 1, 14)
    }
    fn uplink_code() -> RsCode {
        RsCode::new(0x87, 120, 1, 20)
    }

    fn roundtrip(code: &RsCode, k: usize, max_errors: usize, seed: u64) {
        let mut x = seed;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let data: Vec<u8> = (0..k).map(|_| next() as u8).collect();
        let parity = code.encode(&data);
        let mut codeword: Vec<u8> = data.iter().chain(parity.iter()).copied().collect();
        let original = codeword.clone();

        // Corrupt up to max_errors distinct positions.
        let mut positions: Vec<usize> = Vec::new();
        while positions.len() < max_errors {
            let p = (next() as usize) % codeword.len();
            if !positions.contains(&p) {
                positions.push(p);
            }
        }
        for &p in &positions {
            let mut bad = next() as u8;
            if bad == 0 {
                bad = 1;
            }
            codeword[p] ^= bad;
        }

        let corrected = code.decode(&mut codeword);
        assert_eq!(corrected, Some(max_errors), "seed={seed} errors={max_errors}");
        assert_eq!(codeword, original, "seed={seed} errors={max_errors}");
    }

    #[test]
    fn short_code_corrects_up_to_capacity() {
        for seed in 1..30u64 {
            roundtrip(&short_code(), 18, 6, seed); // nroots=12 -> corrects up to 6
        }
    }

    #[test]
    fn long_code_corrects_up_to_capacity() {
        for seed in 1..30u64 {
            roundtrip(&long_code(), 34, 7, seed); // nroots=14 -> corrects up to 7
        }
    }

    #[test]
    fn uplink_code_corrects_up_to_capacity() {
        for seed in 1..30u64 {
            roundtrip(&uplink_code(), 72, 10, seed); // nroots=20 -> corrects up to 10
        }
    }

    #[test]
    fn zero_errors_is_a_fast_clean_pass() {
        let code = short_code();
        let data = vec![0xAAu8; 18];
        let parity = code.encode(&data);
        let mut codeword: Vec<u8> = data.iter().chain(parity.iter()).copied().collect();
        assert_eq!(code.decode(&mut codeword), Some(0));
    }

    #[test]
    fn too_many_errors_is_reported_not_miscorrected() {
        let code = short_code(); // corrects up to 6
        let data = vec![0x55u8; 18];
        let parity = code.encode(&data);
        let mut codeword: Vec<u8> = data.iter().chain(parity.iter()).copied().collect();
        for p in 0..9 {
            codeword[p] ^= 0xFF; // 9 errors, past the 6-symbol capacity
        }
        assert_eq!(code.decode(&mut codeword), None);
    }
}
