//! Basic processor implementations.

pub mod exist;
pub mod fsstream;
pub mod glob;
pub mod parser;

pub use exist::CheckExistProcessor;
pub use fsstream::FsstreamProcessor;
pub use glob::TinyGlobbyProcessor;
pub use parser::ParserProcessor;
