//! Basic processor implementations.

pub mod exist;
pub mod glob;
pub mod parser;

pub use exist::CheckExistProcessor;
pub use glob::TinyGlobbyProcessor;
pub use parser::ParserProcessor;
