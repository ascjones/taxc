pub mod cgt;
pub mod income;
pub mod uk;

pub use cgt::calculate_cgt;
pub use income::calculate_income_tax;
pub use uk::{TaxBand, TaxYear};
