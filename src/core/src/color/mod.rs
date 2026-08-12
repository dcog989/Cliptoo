// Color parsing and OKLCH ↔ sRGB conversion.
// See PORTING.md §4 for full algorithm.

pub mod oklch;
pub mod parser;

pub use oklch::{oklch_to_srgb_bytes, srgb_bytes_to_oklch};
pub use parser::{ColorParser, ParsedColor};
