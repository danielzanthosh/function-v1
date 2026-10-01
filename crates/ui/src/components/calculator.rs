//! Built-in instant arithmetic calculator inspired by Flow Launcher's calculator plugin.
//!
//! Evaluates mathematical expressions in real-time as the user types,
//! returning a cleanly formatted result that can be copied or inserted.

#[derive(Debug, PartialEq)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Power,
    LParen,
    RParen,
    Sqrt,
    Abs,
}

struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn next_token(&mut self) -> Option<Result<Token, ()>> {
        self.skip_whitespace();
        if self.pos >= self.input.len() {
            return None;
        }

        let ch = self.input[self.pos];
        match ch {
            b'+' => {
                self.pos += 1;
                Some(Ok(Token::Plus))
            }
            b'-' => {
                self.pos += 1;
                Some(Ok(Token::Minus))
            }
            b'*' | b'x' | b'X' => {
                self.pos += 1;
                Some(Ok(Token::Multiply))
            }
            b'/' => {
                self.pos += 1;
                Some(Ok(Token::Divide))
            }
            b'%' => {
                self.pos += 1;
                Some(Ok(Token::Modulo))
            }
            b'^' => {
                self.pos += 1;
                Some(Ok(Token::Power))
            }
            b'(' => {
                self.pos += 1;
                Some(Ok(Token::LParen))
            }
            b')' => {
                self.pos += 1;
                Some(Ok(Token::RParen))
            }
            b'0'..=b'9' | b'.' => {
                let start = self.pos;
                let mut has_dot = ch == b'.';
                self.pos += 1;
                while self.pos < self.input.len() {
                    let c = self.input[self.pos];
                    if c.is_ascii_digit() {
                        self.pos += 1;
                    } else if c == b'.' && !has_dot {
                        has_dot = true;
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                let s = std::str::from_utf8(&self.input[start..self.pos]).ok()?;
                let val: f64 = s.parse().ok()?;
                Some(Ok(Token::Number(val)))
            }
            b's' | b'S' => {
                if self.input[self.pos..].starts_with(b"sqrt")
                    || self.input[self.pos..].starts_with(b"SQRT")
                {
                    self.pos += 4;
                    Some(Ok(Token::Sqrt))
                } else {
                    None
                }
            }
            b'a' | b'A' => {
                if self.input[self.pos..].starts_with(b"abs")
                    || self.input[self.pos..].starts_with(b"ABS")
                {
                    self.pos += 3;
                    Some(Ok(Token::Abs))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn tokenize(&mut self) -> Option<Vec<Token>> {
        let mut tokens = Vec::new();
        while self.pos < self.input.len() {
            match self.next_token()? {
                Ok(tok) => tokens.push(tok),
                Err(_) => return None,
            }
        }
        if tokens.is_empty() {
            None
        } else {
            Some(tokens)
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        self.pos += 1;
        tok
    }

    fn parse_expression(&mut self) -> Option<f64> {
        let mut val = self.parse_term()?;

        while let Some(tok) = self.current() {
            match tok {
                Token::Plus => {
                    self.advance();
                    val += self.parse_term()?;
                }
                Token::Minus => {
                    self.advance();
                    val -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Some(val)
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut val = self.parse_factor()?;

        while let Some(tok) = self.current() {
            match tok {
                Token::Multiply => {
                    self.advance();
                    val *= self.parse_factor()?;
                }
                Token::Divide => {
                    self.advance();
                    let denom = self.parse_factor()?;
                    if denom == 0.0 {
                        return None;
                    }
                    val /= denom;
                }
                Token::Modulo => {
                    self.advance();
                    let denom = self.parse_factor()?;
                    if denom == 0.0 {
                        return None;
                    }
                    val %= denom;
                }
                _ => break,
            }
        }
        Some(val)
    }

    fn parse_factor(&mut self) -> Option<f64> {
        let base = self.parse_unary()?;
        if let Some(Token::Power) = self.current() {
            self.advance();
            let exp = self.parse_factor()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn parse_unary(&mut self) -> Option<f64> {
        match self.current()? {
            Token::Minus => {
                self.advance();
                Some(-self.parse_unary()?)
            }
            Token::Plus => {
                self.advance();
                self.parse_unary()
            }
            Token::Sqrt => {
                self.advance();
                let inner = self.parse_unary()?;
                if inner < 0.0 {
                    None
                } else {
                    Some(inner.sqrt())
                }
            }
            Token::Abs => {
                self.advance();
                let inner = self.parse_unary()?;
                Some(inner.abs())
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        match self.advance()? {
            Token::Number(n) => Some(*n),
            Token::LParen => {
                let val = self.parse_expression()?;
                if let Some(Token::RParen) = self.advance() {
                    Some(val)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Evaluates a math query if it represents a valid arithmetic expression.
/// Returns None if the input is not a calculation or contains invalid characters.
pub fn evaluate_calculation(input: &str) -> Option<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Require at least one math operator or function to prevent interpreting standalone numbers
    let has_operator = trimmed
        .chars()
        .any(|c| matches!(c, '+' | '-' | '*' | '/' | '%' | '^' | 'x' | 'X'))
        || trimmed.to_lowercase().contains("sqrt")
        || trimmed.to_lowercase().contains("abs");

    if !has_operator {
        return None;
    }

    let mut lexer = Lexer::new(trimmed);
    let tokens = lexer.tokenize()?;

    // Require at least one number and operator
    let has_number = tokens.iter().any(|t| matches!(t, Token::Number(_)));
    if !has_number {
        return None;
    }

    let mut parser = Parser::new(tokens);
    let result = parser.parse_expression()?;

    // Must have consumed all tokens
    if parser.pos == parser.tokens.len() && !result.is_nan() && !result.is_infinite() {
        Some(result)
    } else {
        None
    }
}

/// Format calculated result cleanly (e.g. integer if whole, otherwise rounded to 6 decimal places).
pub fn format_result(val: f64) -> String {
    if (val.fract()).abs() < 1e-9 {
        format!("{:.0}", val)
    } else {
        let s = format!("{:.6}", val);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_arithmetic() {
        assert_eq!(evaluate_calculation("2 + 2"), Some(4.0));
        assert_eq!(evaluate_calculation("12 * 8"), Some(96.0));
        assert_eq!(evaluate_calculation("100 / 4"), Some(25.0));
        assert_eq!(evaluate_calculation("50 - 18"), Some(32.0));
    }

    #[test]
    fn test_precedence_and_parentheses() {
        assert_eq!(evaluate_calculation("2 + 3 * 4"), Some(14.0));
        assert_eq!(evaluate_calculation("(2 + 3) * 4"), Some(20.0));
        assert_eq!(evaluate_calculation("2 ^ 3"), Some(8.0));
        assert_eq!(evaluate_calculation("sqrt(16)"), Some(4.0));
    }

    #[test]
    fn test_invalid_expressions() {
        assert_eq!(evaluate_calculation("hello world"), None);
        assert_eq!(evaluate_calculation("123"), None); // Standalone number shouldn't trigger calculator
        assert_eq!(evaluate_calculation("> dir"), None);
    }

    #[test]
    fn test_formatting() {
        assert_eq!(format_result(4.0), "4");
        assert_eq!(format_result(3.140000), "3.14");
    }
}
