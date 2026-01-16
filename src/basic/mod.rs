//! Basic processor implementations.

pub mod parser;
pub mod exist;
pub mod glob;

pub use parser::ParserProcessor;
pub use exist::CheckExistProcessor;
// pub use glob::TinyGlobbyProcessor;
