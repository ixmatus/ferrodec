macro_rules! show { ($T:ty, $tag:expr) => {{
    for (n, v) in [("ZERO", <$T>::ZERO), ("NEG_ZERO", <$T>::NEG_ZERO), ("ONE", <$T>::ONE), ("NEG_ONE", <$T>::NEG_ONE), ("TEN", <$T>::TEN), ("MAX", <$T>::MAX), ("MIN", <$T>::MIN), ("MIN_POSITIVE", <$T>::MIN_POSITIVE), ("MIN_POSITIVE_NORMAL", <$T>::MIN_POSITIVE_NORMAL), ("NAN", <$T>::NAN), ("SIGNALING_NAN", <$T>::SIGNALING_NAN), ("INFINITY", <$T>::INFINITY), ("NEG_INFINITY", <$T>::NEG_INFINITY)] {
        println!("{} {} {:e} canonical={} {:?}", $tag, n, v, v.is_canonical(), v.decode());
    }
}}}
fn main() { show!(ferrodec_decimal64::Decimal64, "d64"); show!(ferrodec_decimal32::Decimal32, "d32"); }
