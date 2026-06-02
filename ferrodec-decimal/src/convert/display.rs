//! Formatting a [`Decimal`] in General Decimal Arithmetic to-scientific
//! notation, the canonical string form (specification §"to-scientific-string").

use crate::Decimal;
use alloc::string::ToString;
use core::fmt;

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((sign, signaling, payload)) = self.nan_parts() {
            if sign {
                f.write_str("-")?;
            }
            f.write_str(if signaling { "sNaN" } else { "NaN" })?;
            if !payload.is_zero() {
                write!(f, "{payload}")?;
            }
            return Ok(());
        }
        if self.is_infinite() {
            if self.is_negative() {
                f.write_str("-")?;
            }
            return f.write_str("Infinity");
        }

        let (sign, coeff, exp) = self.finite_parts().expect("finite after specials");
        if sign {
            f.write_str("-")?;
        }
        write_scientific(f, &coeff.to_string(), exp)
    }
}

/// Format a finite magnitude given its coefficient digit string `c` (no
/// leading zeros, `"0"` for zero) and exponent `exp`.
///
/// Per the specification, let `adjexp = exp + len(c) - 1`. Plain notation is
/// used when `exp <= 0 && adjexp >= -6`; otherwise scientific notation places
/// the point after the first digit and appends `E` with the signed adjusted
/// exponent.
fn write_scientific(f: &mut fmt::Formatter<'_>, c: &str, exp: i32) -> fmt::Result {
    let len = c.len() as i64;
    let adjexp = i64::from(exp) + len - 1;

    if exp <= 0 && adjexp >= -6 {
        if exp == 0 {
            return f.write_str(c);
        }
        // Position of the decimal point measured from the start of `c`.
        let point = len + i64::from(exp);
        if point > 0 {
            let point = point as usize;
            f.write_str(&c[..point])?;
            f.write_str(".")?;
            f.write_str(&c[point..])
        } else {
            f.write_str("0.")?;
            for _ in 0..-point {
                f.write_str("0")?;
            }
            f.write_str(c)
        }
    } else {
        f.write_str(&c[..1])?;
        if len > 1 {
            f.write_str(".")?;
            f.write_str(&c[1..])?;
        }
        f.write_str("E")?;
        if adjexp >= 0 {
            f.write_str("+")?;
        }
        write!(f, "{adjexp}")
    }
}
