#![no_std]
#![no_main]
use core::panic::PanicInfo;
use ferrodec::Decimal128;
#[panic_handler]
fn panic(_: &PanicInfo) -> ! { loop {} }
#[no_mangle]
pub extern "C" fn _start() -> u32 {
    let d = core::hint::black_box(Decimal128::ONE);
    let (r, _) = d.add(d, ferrodec::RoundingMode::NearestEven);
    r.to_bits() as u32
}
