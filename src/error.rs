#[derive(Debug)]
pub enum NesError {
    GenericError,
}

impl std::fmt::Display for NesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NesError::GenericError => write!(f, "GenericError"),
        }
    }
}

impl std::error::Error for NesError {}
