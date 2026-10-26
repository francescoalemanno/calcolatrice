//! Lexer e parser a discesa ricorsiva per formule matematiche.
//!
//! Grammatica (dalla precedenza più bassa alla più alta):
//!   statement  := expr ('=' expr)?
//!   comparison := additive (('<' | '>' | '<=' | '>=') additive)?
//!   additive   := term (('+' | '-') term)*      (a ± b% = a·(1 ± b/100))
//!   term       := unary (('*' | '/' | '%') unary | <moltiplicazione implicita> unary)*
//!   unary      := ('-' | '+') unary | power
//!   power      := postfix ('^' unary)?
//!   postfix    := primary ('!' | '%')*            ('%' è percentuale se non segue un operando)
//!   primary    := numero | ident | ident '(' args ')' | '(' expr (',' expr)* ')'
//!               | '|' expr '|' | '√' postfix

use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Bang,
    LParen,
    RParen,
    Comma,
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    Bar,
    Sqrt,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Num(n) => write!(f, "{n}"),
            Token::Ident(s) => write!(f, "{s}"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "−"),
            Token::Star => write!(f, "×"),
            Token::Slash => write!(f, "÷"),
            Token::Percent => write!(f, "%"),
            Token::Caret => write!(f, "^"),
            Token::Bang => write!(f, "!"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::Comma => write!(f, ","),
            Token::Eq => write!(f, "="),
            Token::Lt => write!(f, "<"),
            Token::Gt => write!(f, ">"),
            Token::Le => write!(f, "≤"),
            Token::Ge => write!(f, "≥"),
            Token::Bar => write!(f, "|"),
            Token::Sqrt => write!(f, "√"),
        }
    }
}

/// Caratteri ammessi dopo il primo in un nome. Solo cifre ASCII: '²' e '³' per Rust
/// sono alfanumerici, ma qui sono potenze (x² = x^2).
fn is_ident_char(c: char) -> bool {
    c.is_alphabetic() || c.is_ascii_digit() || c == '_' || c == '\''
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // Esponente scientifico: 1e-3, 2.5E+10
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    i = j;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let s: String = chars[start..i].iter().collect();
            let n: f64 = s.parse().map_err(|_| format!("numero non valido: {s}"))?;
            out.push(Token::Num(n));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let s = match s.as_str() {
                "π" => "pi".to_string(),
                "θ" => "theta".to_string(),
                "τ" => "tau".to_string(),
                "φ" => "phi".to_string(),
                _ => s,
            };
            out.push(Token::Ident(s));
            continue;
        }
        let next = chars.get(i + 1).copied();
        let (tok, len) = match c {
            '+' => (Token::Plus, 1),
            '-' | '−' => (Token::Minus, 1),
            '*' if next == Some('*') => (Token::Caret, 2),
            '*' | '×' | '·' => (Token::Star, 1),
            '/' | '÷' => (Token::Slash, 1),
            '%' => (Token::Percent, 1),
            '^' => (Token::Caret, 1),
            '!' => (Token::Bang, 1),
            '(' | '[' => (Token::LParen, 1),
            ')' | ']' => (Token::RParen, 1),
            ',' | ';' => (Token::Comma, 1),
            '=' if next == Some('=') => (Token::Eq, 2),
            '=' => (Token::Eq, 1),
            '<' if next == Some('=') => (Token::Le, 2),
            '>' if next == Some('=') => (Token::Ge, 2),
            '<' => (Token::Lt, 1),
            '>' => (Token::Gt, 1),
            '≤' => (Token::Le, 1),
            '≥' => (Token::Ge, 1),
            '|' => (Token::Bar, 1),
            '√' => (Token::Sqrt, 1),
            '²' => {
                out.push(Token::Caret);
                (Token::Num(2.0), 1)
            }
            '³' => {
                out.push(Token::Caret);
                (Token::Num(3.0), 1)
            }
            _ => return Err(format!("carattere non riconosciuto: '{c}'")),
        };
        out.push(tok);
        i += len;
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    Var(String),
    Neg(Box<Expr>),
    Fact(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Tuple(Vec<Expr>),
}

impl Expr {
    fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
        Expr::Bin(op, Box::new(a), Box::new(b))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Expr(Expr),
    /// `nome = expr`
    Assign(String, Expr),
    /// `f(a, b) = expr`
    FuncDef(String, Vec<String>, Expr),
    /// `lhs = rhs` generica
    Equation(Expr, Expr),
}

/// Funzioni note al parser: un identificatore seguito da `(` è una chiamata
/// solo se è una funzione, altrimenti è una moltiplicazione implicita (`a(x+1)`).
pub fn is_builtin_function(name: &str) -> bool {
    crate::eval::builtin_arity(name).is_some()
}

struct Parser<'a> {
    toks: Vec<Token>,
    pos: usize,
    user_funcs: &'a HashSet<String>,
    /// Posizione subito dopo l'ultimo '%' letto come percentuale.
    pct_end: Option<usize>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn eat(&mut self, t: &Token) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, t: &Token) -> Result<(), String> {
        match self.next() {
            Some(ref got) if got == t => Ok(()),
            Some(got) => Err(format!("atteso '{t}', trovato '{got}'")),
            None => Err(format!("atteso '{t}' alla fine")),
        }
    }

    fn is_function(&self, name: &str) -> bool {
        is_builtin_function(name) || self.user_funcs.contains(name)
    }

    fn comparison(&mut self) -> Result<Expr, String> {
        let lhs = self.additive()?;
        let op = match self.peek() {
            Some(Token::Lt) => BinOp::Lt,
            Some(Token::Gt) => BinOp::Gt,
            Some(Token::Le) => BinOp::Le,
            Some(Token::Ge) => BinOp::Ge,
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.additive()?;
        Ok(Expr::bin(op, lhs, rhs))
    }

    fn additive(&mut self) -> Result<Expr, String> {
        let mut lhs = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinOp::Add,
                Some(Token::Minus) => BinOp::Sub,
                _ => return Ok(lhs),
            };
            self.pos += 1;
            let rhs = self.term()?;
            // Come sulle calcolatrici: 100 - 10% = 90, 50 + 20% = 60
            if self.pct_end == Some(self.pos) && matches!(&rhs, Expr::Bin(BinOp::Div, _, d) if **d == Expr::Num(100.0)) {
                lhs = Expr::bin(BinOp::Mul, lhs.clone(), Expr::bin(op, Expr::Num(1.0), rhs));
                continue;
            }
            lhs = Expr::bin(op, lhs, rhs);
        }
    }

    fn starts_primary(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::Num(_) | Token::Ident(_) | Token::LParen | Token::Sqrt)
        )
    }

    fn term(&mut self) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinOp::Mul,
                Some(Token::Slash) => BinOp::Div,
                Some(Token::Percent) => BinOp::Mod,
                _ if self.starts_primary() => {
                    // Moltiplicazione implicita: 2x, 3(x+1), x sin(x)
                    let rhs = self.power()?;
                    lhs = Expr::bin(BinOp::Mul, lhs, rhs);
                    continue;
                }
                _ => return Ok(lhs),
            };
            self.pos += 1;
            let rhs = self.unary()?;
            lhs = Expr::bin(op, lhs, rhs);
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat(&Token::Minus) {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat(&Token::Plus) {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Result<Expr, String> {
        let base = self.postfix()?;
        if self.eat(&Token::Caret) {
            let exp = self.unary()?;
            return Ok(Expr::bin(BinOp::Pow, base, exp));
        }
        Ok(base)
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        loop {
            if self.eat(&Token::Bang) {
                e = Expr::Fact(Box::new(e));
            } else if self.peek() == Some(&Token::Percent)
                && !matches!(
                    self.toks.get(self.pos + 1),
                    Some(Token::Num(_) | Token::Ident(_) | Token::LParen | Token::Sqrt)
                )
            {
                // '%' senza operando a destra: percentuale (altrimenti è modulo)
                self.pos += 1;
                self.pct_end = Some(self.pos);
                e = Expr::bin(BinOp::Div, e, Expr::Num(100.0));
            } else {
                return Ok(e);
            }
        }
    }

    fn args(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        if self.eat(&Token::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.comparison()?);
            if self.eat(&Token::Comma) {
                continue;
            }
            self.expect(&Token::RParen)?;
            return Ok(args);
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::Num(n)) => Ok(Expr::Num(n)),
            Some(Token::Ident(name)) => {
                if self.peek() == Some(&Token::LParen) && self.is_function(&name) {
                    self.pos += 1;
                    let args = self.args()?;
                    Ok(Expr::Call(name, args))
                } else if self.is_function(&name) && self.starts_primary() {
                    // sin x, ln 2  →  sin(x), ln(2)
                    let arg = self.power()?;
                    Ok(Expr::Call(name, vec![arg]))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Some(Token::LParen) => {
                let first = self.comparison()?;
                if self.eat(&Token::Comma) {
                    let mut items = vec![first];
                    loop {
                        items.push(self.comparison()?);
                        if !self.eat(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect(&Token::RParen)?;
                    return Ok(Expr::Tuple(items));
                }
                self.expect(&Token::RParen)?;
                Ok(first)
            }
            Some(Token::Bar) => {
                let inner = self.comparison()?;
                self.expect(&Token::Bar)?;
                Ok(Expr::Call("abs".into(), vec![inner]))
            }
            Some(Token::Sqrt) => {
                let inner = self.postfix()?;
                Ok(Expr::Call("sqrt".into(), vec![inner]))
            }
            Some(t) => Err(format!("simbolo inatteso '{t}'")),
            None => Err("espressione incompleta".into()),
        }
    }
}

/// Riconosce `nome(a, b, ...) =` all'inizio della riga.
fn func_def_header(toks: &[Token]) -> Option<(String, Vec<String>, usize)> {
    let Some(Token::Ident(name)) = toks.first() else { return None };
    if toks.get(1) != Some(&Token::LParen) || is_builtin_function(name) {
        return None;
    }
    let mut params = Vec::new();
    let mut i = 2;
    loop {
        match toks.get(i) {
            Some(Token::Ident(p)) => params.push(p.clone()),
            _ => return None,
        }
        i += 1;
        match toks.get(i) {
            Some(Token::Comma) => i += 1,
            Some(Token::RParen) => break,
            _ => return None,
        }
    }
    (toks.get(i + 1) == Some(&Token::Eq)).then(|| (name.clone(), params, i + 2))
}

/// Restituisce il nome della funzione se la riga è una definizione `f(x) = ...`.
pub fn peek_func_def(src: &str) -> Option<String> {
    let toks = tokenize(src).ok()?;
    func_def_header(&toks).map(|(name, _, _)| name)
}

pub fn parse_statement(src: &str, user_funcs: &HashSet<String>) -> Result<Statement, String> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err("vuoto".into());
    }
    if let Some((name, params, body_start)) = func_def_header(&toks) {
        let mut p = Parser { toks: toks[body_start..].to_vec(), pos: 0, user_funcs, pct_end: None };
        let body = p.comparison()?;
        if p.pos < p.toks.len() {
            return Err(format!("simbolo inatteso '{}'", p.toks[p.pos]));
        }
        return Ok(Statement::FuncDef(name, params, body));
    }
    let mut p = Parser { toks, pos: 0, user_funcs, pct_end: None };
    let lhs = p.comparison()?;
    let stmt = if p.eat(&Token::Eq) {
        let rhs = p.comparison()?;
        match lhs {
            Expr::Var(name) => Statement::Assign(name, rhs),
            lhs => Statement::Equation(lhs, rhs),
        }
    } else {
        Statement::Expr(lhs)
    };
    if p.pos < p.toks.len() {
        return Err(format!("simbolo inatteso '{}'", p.toks[p.pos]));
    }
    Ok(stmt)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Statement {
        parse_statement(s, &HashSet::new()).unwrap()
    }

    #[test]
    fn superscript_powers_after_names() {
        let sq = |v: &str| Statement::Expr(Expr::bin(BinOp::Pow, Expr::Var(v.into()), Expr::Num(2.0)));
        assert_eq!(parse("y²"), sq("y"));
        assert_eq!(parse("theta²"), sq("theta"));
        assert_eq!(parse("x2"), Statement::Expr(Expr::Var("x2".into())));
    }

    #[test]
    fn implicit_multiplication() {
        assert_eq!(
            parse("2x"),
            Statement::Expr(Expr::bin(BinOp::Mul, Expr::Num(2.0), Expr::Var("x".into())))
        );
        assert!(matches!(parse("2 sin(x)"), Statement::Expr(Expr::Bin(BinOp::Mul, _, _))));
        assert!(matches!(parse("(x+1)(x-1)"), Statement::Expr(Expr::Bin(BinOp::Mul, _, _))));
    }

    #[test]
    fn unary_minus_binds_looser_than_power() {
        assert_eq!(
            parse("-x^2"),
            Statement::Expr(Expr::Neg(Box::new(Expr::bin(
                BinOp::Pow,
                Expr::Var("x".into()),
                Expr::Num(2.0)
            ))))
        );
    }

    #[test]
    fn statements() {
        assert!(matches!(parse("a = 3"), Statement::Assign(..)));
        assert!(matches!(parse("f(x, y) = x*y"), Statement::FuncDef(..)));
        assert!(matches!(parse("x^2 + y^2 = 1"), Statement::Equation(..)));
        assert!(matches!(parse("(cos t, sin t)"), Statement::Expr(Expr::Tuple(_))));
    }

    #[test]
    fn errors() {
        assert!(parse_statement("2 +", &HashSet::new()).is_err());
        assert!(parse_statement("(1", &HashSet::new()).is_err());
        assert!(parse_statement("1 $ 2", &HashSet::new()).is_err());
    }
}
