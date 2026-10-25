use super::parser::Parser;
use super::ParseError;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Lhs {
    DollarSign(usize, usize),
    Amp(usize, usize),
    At(usize, Box<Rhs>),
    Square(String),
    /// Bunch of star expressions separated by pipes
    Pipes(Vec<Stars>),
    Literal(String),
}

impl Lhs {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        Parser::new(input).parse_lhs()
    }
}

/// Bunch of literals separated by stars
/// "*" is represented as vec!["", ""]
#[derive(Debug, PartialEq, Clone, Eq)]
pub struct Stars(pub Vec<String>);

#[derive(Debug, PartialEq, Clone, Eq)]
pub struct Rhs(pub Vec<RhsPart>);

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum RhsPart {
    Index(IndexOp),
    CompositeKey(Vec<RhsEntry>),
    Key(RhsEntry),
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum RhsEntry {
    Amp(usize, usize),
    At(usize, Box<Rhs>),
    Key(String),
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum IndexOp {
    Amp(usize, usize),
    Literal(usize),
    At(usize, Box<Rhs>),
    /// `[#n]`: the number of matches made so far by the spec node `n` levels up, counting the
    /// current leaf as level 0. Used to turn a map into a dense array.
    Hash(usize),
    Empty,
}

impl Rhs {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        Parser::new(input).parse_rhs()
    }
}
