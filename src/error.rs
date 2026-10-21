#[derive(Debug)]
pub enum NesError {
    GenericError,
    /// iNES mapper number not implemented yet.
    UnsupportedMapper(u8),
}

impl std::fmt::Display for NesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NesError::GenericError => write!(f, "GenericError"),
            NesError::UnsupportedMapper(id) => {
                write!(f, "unsupported mapper {id} (add src/cart/mapper/…)")
            }
        }
    }
}

impl std::error::Error for NesError {}
