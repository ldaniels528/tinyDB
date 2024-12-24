#![warn(dead_code)]
////////////////////////////////////////////////////////////////////
// token slice module - responsible for parsing language tokens
////////////////////////////////////////////////////////////////////

use std::fmt::Display;
use std::ops::Index;

use serde::{Deserialize, Serialize};

use shared_lib::fail;

use crate::tokenizer;
use crate::tokens::Token;

/// TokenSlice is an immutable navigable sequence of tokens.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct TokenSlice {
    tokens: Vec<Token>,
    pos: isize,
}

impl TokenSlice {
    ////////////////////////////////////////////////////////////////
    // static methods
    ////////////////////////////////////////////////////////////////

    /// Creates a new Token Slice via a vector of tokens.
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Creates a new Token Slice from a string.
    pub fn from_string(text: &str) -> Self {
        Self::new(tokenizer::parse_fully(text))
    }

    ////////////////////////////////////////////////////////////////
    // instance methods
    ////////////////////////////////////////////////////////////////

    pub fn capture(&self, start: &str, end: &str, delim: Option<&str>) -> (Vec<Token>, Self) {
        let inputs = &self.tokens;
        let mut pos = self.pos;
        let mut tokens = Vec::new();
        if inputs[pos as usize].get_raw_value() == start {
            pos += 1;
            while (pos < inputs.len() as isize) && inputs[pos as usize].get_raw_value() != end {
                tokens.push(inputs[pos as usize].to_owned());
                pos += 1;
                match delim {
                    Some(_delim) if pos < inputs.len() as isize && inputs[pos as usize].get_raw_value() == _delim => pos += 1,
                    _ => {}
                }
            }
        }
        (tokens, self.copy(pos))
    }

    /// Creates a new Token Slice via a vector of tokens.
    pub fn copy(&self, pos: isize) -> Self {
        Self { tokens: self.tokens.to_owned(), pos }
    }

    pub fn exists(&self, f: fn(&Token) -> bool) -> bool {
        match self.get() {
            Some(token) => f(token),
            None => false
        }
    }

    pub fn expect(&self, term: &str) -> std::io::Result<Self> {
        if let (Some(tok), ts) = self.next() {
            if tok.contains(term) { Ok(ts) } else {
                fail(format!("Expected '{}' but got '{}'", term, tok))
            }
        } else {
            fail(format!("Expected '{}'", term))
        }
    }

    pub fn fold<A>(&self, init: A, f: fn(&A, &TokenSlice) -> (A, TokenSlice)) -> A {
        let mut result = init;
        let mut a_ts = self.to_owned();
        while a_ts.has_more() {
            let (result1, ts1) = f(&result, &a_ts);
            result = result1;
            a_ts = ts1;
        }
        result
    }

    /// Returns the option of a Token at the current position within the slice.
    pub fn get(&self) -> Option<&Token> {
        if self.pos < self.tokens.len() as isize {
            return Some(&self.tokens[self.pos as usize]);
        }
        None
    }

    /// Returns the position (index) within the slice.
    pub fn get_position(&self) -> isize { self.pos }

    /// Indicates whether at least one more token remains before the end of the slice.
    pub fn has_more(&self) -> bool {
        self.has_next() || (self.get_position() < self.len() as isize)
    }

    /// Indicates whether at least one more token remains before the end of the slice.
    pub fn has_additional(&self) -> bool {
        self.has_next() || (!self.is_empty() && self.get_position() == 0
            && (self.get_position() < self.len() as isize))
    }

    /// Indicates whether there is another token after the current token
    pub fn has_next(&self) -> bool { self.pos + 1 < self.tokens.len() as isize }

    /// Indicates whether at least one more token remains before the beginning of the slice.
    pub fn has_previous(&self) -> bool { self.pos > 0 }

    pub fn is(&self, text: &str) -> bool {
        match self.get() {
            Some(tok) => tok.get_raw_value() == text,
            None => false
        }
    }

    pub fn isnt(&self, text: &str) -> bool { !self.is(text) }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn is_next(&self, f: fn(Token) -> bool) -> bool {
        self.next().0.map_or(false, |t| f(t))
    }

    pub fn is_previous(&self, f: fn(Token) -> bool) -> bool {
        self.previous().0.map_or(false, |t| f(t))
    }

    pub fn is_previous_adjacent(&self) -> bool {
        if self.pos > 0 {
            let tokens = &self.tokens;
            let p1 = self.pos as usize;
            let (t0, t1) = (tokens[p1 - 1].to_owned(), tokens[p1].to_owned());
            t0.get_line_number() == t1.get_line_number() &&
                t0.get_column_number() + t0.get_raw_value().len() == t1.get_column_number()
        } else { false }
    }

    pub fn is_same_line_as_previous(&self) -> bool {
        if self.pos > 0 {
            let p1 = self.pos as usize;
            let p0 = p1 - 1;
            let tokens = &self.tokens;
            tokens[p0].get_line_number() == tokens[p1].get_line_number()
        } else { false }
    }

    pub fn len(&self) -> usize { self.tokens.len() }

    /// Returns the option of a Token at the next position within the slice.
    pub fn next(&self) -> (Option<Token>, Self) {
        let n = self.pos;
        if n < self.tokens.len() as isize {
            return (Some(self[n as usize].to_owned()), self.copy(n + 1));
        }
        (None, self.to_owned())
    }

    pub fn peek(&self) -> Option<&Token> {
        if self.pos >= 0 && self.pos < self.tokens.len() as isize {
            Some(&self.tokens[self.pos as usize])
        } else { None }
    }

    /// Returns the option of a Token at the previous position within the slice.
    pub fn previous(&self) -> (Option<Token>, Self) {
        let n = self.pos - 1;
        if n >= 0 {
            return (Some(self[n as usize].to_owned()), self.copy(n));
        }
        (None, self.to_owned())
    }

    /// Scans the slice moving the cursor forward until the desired match is found.
    /// However, if the end of the sequence is reached before the token is found
    /// there is no effect.
    pub fn scan_to(&self, f: fn(&Token) -> bool) -> (&[Token], Self) {
        let mut pos = self.pos;
        while pos < self.tokens.len() as isize && !f(&self.tokens[pos as usize]) { pos += 1 }
        if pos > self.pos && pos < self.tokens.len() as isize {
            (&self.tokens[self.pos as usize..pos as usize], self.copy(pos))
        } else { (&[], self.to_owned()) }
    }

    /// Scans the slice moving the cursor forward until the desired match is found.
    /// However, if the end of the sequence is reached before the token is found
    /// there is no effect.
    pub fn scan_until(&self, f: fn(&Token) -> bool) -> (&[Token], Self) {
        let mut pos = self.pos;
        while pos < self.tokens.len() as isize && !f(&self.tokens[pos as usize]) { pos += 1 }
        if pos > self.pos && pos < self.tokens.len() as isize {
            (&self.tokens[self.pos as usize..=pos as usize], self.copy(pos))
        } else { (&[], self.to_owned()) }
    }

    pub fn skip(&self) -> Self { self.next().1 }

    pub fn tail(&self) -> Self { Self::new(self.tokens[(self.pos + 1) as usize..self.len()].to_vec()) }

    pub fn while_do<A>(
        &self,
        cond: fn(&TokenSlice) -> bool,
        f: fn(TokenSlice
        ) -> std::io::Result<TokenSlice>) -> std::io::Result<TokenSlice> {
        let mut a_ts = self.to_owned();
        while cond(&a_ts) {
            a_ts = f(a_ts)?
        }
        Ok(a_ts)
    }
}

impl Display for TokenSlice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get().map(|t| t.get_raw_value()).unwrap_or("".into()))
    }
}

impl Index<usize> for TokenSlice {
    type Output = Token;

    fn index(&self, index: usize) -> &Self::Output {
        &self.tokens[index]
    }
}

// Unit tests
#[cfg(test)]
mod tests {
    use crate::tokens::Token::*;

    use super::*;

    #[test]
    fn test_capture_with_delimiter() {
        let ts = TokenSlice::from_string("(123, 'Hello', abc)");
        let (tokens, _) = ts.capture("(", ")", Some(","));
        assert_eq!(tokens, vec![
            Token::numeric("123".into(), 1, 4, 1, 3),
            Token::single_quoted("Hello".into(), 7, 12, 1, 9),
            Token::atom("abc".into(), 15, 18, 1, 17),
        ])
    }

    #[test]
    fn test_capture_without_delimiter() {
        let ts = TokenSlice::from_string("(123, 'Hello', abc)");
        let (tokens, _) = ts.capture("(", ")", None);
        assert_eq!(tokens, vec![
            Token::numeric("123".into(), 1, 4, 1, 3),
            Token::operator(",".into(), 4, 5, 1, 6),
            Token::single_quoted("Hello".into(), 7, 12, 1, 9),
            Token::operator(",".into(), 13, 14, 1, 15),
            Token::atom("abc".into(), 15, 18, 1, 17),
        ])
    }

    #[test]
    fn test_cursor_current_next_and_previous() {
        let ts = TokenSlice::from_string("123, Hello World");
        assert!(ts.has_next());
        assert_eq!(ts.get(), Some(&Token::numeric("123".into(), 0, 3, 1, 2)));
        let (row, ts) = ts.next();
        assert_eq!(row, Some(Token::numeric("123".into(), 0, 3, 1, 2)));
        let (row, ts) = ts.next();
        assert_eq!(row, Some(Token::operator(",".into(), 3, 4, 1, 5)));
        let (row, ts) = ts.next();
        assert_eq!(row, Some(Token::atom("Hello".into(), 5, 10, 1, 7)));
        let (row, ts) = ts.next();
        assert_eq!(row, Some(Token::atom("World".into(), 11, 16, 1, 13)));
        let (row, ts) = ts.next();
        assert_eq!(row, None);
        assert_eq!(ts.get_position(), ts.len() as isize);
        assert!(ts.has_previous());
        let (row, ts) = ts.previous();
        assert_eq!(row, Some(Token::atom("World".into(), 11, 16, 1, 13)));
        let (row, ts) = ts.previous();
        assert_eq!(row, Some(Token::atom("Hello".into(), 5, 10, 1, 7)));
        let (row, ts) = ts.previous();
        assert_eq!(row, Some(Token::operator(",".into(), 3, 4, 1, 5)));
        let (row, ts) = ts.previous();
        assert_eq!(row, Some(Token::numeric("123".into(), 0, 3, 1, 2)));
        let (row, _) = ts.previous();
        assert_eq!(row, None);
    }

    #[test]
    fn test_exists() {
        let ts = TokenSlice::from_string("'Hello World'");
        assert!(ts.exists(|t| t.is_single_quoted()));
    }

    #[test]
    fn test_indexing_into() {
        let ts = TokenSlice::from_string("the little brown fox");
        assert_eq!(ts[1], Token::atom("little".into(), 4, 10, 1, 6))
    }

    #[test]
    fn test_is_text_match() {
        let ts = TokenSlice::from_string("abc; 'Hello World'");
        assert!(ts.is("abc"));
    }

    #[test]
    fn test_is_empty() {
        let ts = TokenSlice::from_string("");
        assert!(ts.is_empty());
    }

    #[test]
    fn test_is_previous_adjacent() {
        let ts = TokenSlice::from_string("students[3]");
        let (tok0, ts) = ts.next();
        assert_eq!(tok0.map(|t| t.get_raw_value()), Some("students".to_string()));
        assert!(ts.is_previous_adjacent());
    }

    #[test]
    fn test_is_not_previous_adjacent() {
        let ts = TokenSlice::from_string("students [3]");
        let (tok0, ts) = ts.next();
        assert_eq!(tok0.map(|t| t.get_raw_value()), Some("students".to_string()));
        assert!(!ts.is_previous_adjacent());
    }

    #[test]
    fn test_is_same_line_as_previous() {
        let ts = TokenSlice::from_string("students[3]");
        let (tok0, ts) = ts.next();
        assert_eq!(tok0.map(|t| t.get_raw_value()), Some("students".to_string()));
        assert!(ts.is_same_line_as_previous());
    }

    #[test]
    fn test_is_not_same_line_as_previous() {
        let ts = TokenSlice::from_string(r#"
        items
        [3]
        "#);
        let (tok0, ts) = ts.next();
        assert_eq!(tok0.map(|t| t.get_raw_value()), Some("items".to_string()));
        assert!(!ts.is_same_line_as_previous());
    }

    #[test]
    fn test_scan_to() {
        let ts = TokenSlice::from_string("the fox was too 'fast!' for me");
        let (rows, ts) = ts.scan_to(|t| t.is_single_quoted());
        assert_eq!(rows, &[
            Token::atom("the".into(), 0, 3, 1, 2),
            Token::atom("fox".into(), 4, 7, 1, 6),
            Token::atom("was".into(), 8, 11, 1, 10),
            Token::atom("too".into(), 12, 15, 1, 14)
        ]);
        assert_eq!(ts.get_position(), 4);
    }

    #[test]
    fn test_scan_until() {
        let ts = TokenSlice::from_string("the fox was too 'fast!' for me");
        let (rows, ts) = ts.scan_until(|t| t.is_single_quoted());
        assert_eq!(rows, &[
            Token::atom("the".into(), 0, 3, 1, 2),
            Token::atom("fox".into(), 4, 7, 1, 6),
            Token::atom("was".into(), 8, 11, 1, 10),
            Token::atom("too".into(), 12, 15, 1, 14),
            Token::single_quoted("fast!".into(), 17, 22, 1, 19)
        ]);
        assert_eq!(ts.get_position(), 4);
    }

    #[test]
    fn test_tail() {
        let ts = TokenSlice::from_string("abc 123 def 456");
        assert_eq!(ts.tail(), TokenSlice::new(vec![
            Numeric { text: "123".into(), start: 4, end: 7, line_number: 1, column_number: 6 },
            Atom { text: "def".into(), start: 8, end: 11, line_number: 1, column_number: 10 },
            Numeric { text: "456".into(), start: 12, end: 15, line_number: 1, column_number: 14 },
        ]))
    }
}