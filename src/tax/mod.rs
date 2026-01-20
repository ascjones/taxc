pub mod cgt;
pub mod income;
pub mod uk;

pub use cgt::{calculate_cgt, CgtReport};
pub use income::{calculate_income_tax, IncomeReport};
pub use uk::{TaxBand, TaxYear};
