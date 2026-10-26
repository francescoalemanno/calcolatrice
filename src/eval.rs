//! Compilazione delle espressioni in un albero a "slot" e valutazione numerica.
//!
//! Le variabili libere (non definite) diventano slot che il chiamante riempie:
//! gli assi del grafico (x, y, t, θ) o i parametri regolabili con slider.

use std::collections::HashMap;
use std::f64::consts::{E, PI, TAU};

use crate::parser::{BinOp, Expr};

const MAX_INLINE_DEPTH: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arity {
    Exact(usize),
    Range(usize, usize),
    AtLeast(usize),
}

impl Arity {
    fn accepts(self, n: usize) -> bool {
        match self {
            Arity::Exact(k) => n == k,
            Arity::Range(a, b) => (a..=b).contains(&n),
            Arity::AtLeast(k) => n >= k,
        }
    }
}

pub fn builtin_arity(name: &str) -> Option<Arity> {
    use Arity::*;
    Some(match name {
        "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "asin" | "acos" | "atan" | "arcsin"
        | "arccos" | "arctan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh" | "exp"
        | "ln" | "log2" | "log10" | "sqrt" | "cbrt" | "abs" | "floor" | "ceil" | "round"
        | "trunc" | "sign" | "sgn" | "gamma" | "fact" | "erf" | "erfc" | "frac" | "deg"
        | "rad" => Exact(1),
        "log" => Range(1, 2),
        "atan2" | "pow" | "hypot" | "mod" | "gcd" | "lcm" | "binom" | "ncr" | "npr" | "root" => {
            Exact(2)
        }
        "min" | "max" | "mean" => AtLeast(1),
        "if" => Range(2, 3),
        "diff" | "derivative" => Range(2, 3),
        "integral" | "integrate" | "int" | "sum" | "prod" => Exact(4),
        _ => return None,
    })
}

/// Elenco usato dall'interfaccia per l'aiuto.
pub const FUNCTION_HELP: &[(&str, &str)] = &[
    ("sin cos tan cot sec csc", "trigonometriche"),
    ("asin acos atan atan2(y,x)", "inverse"),
    ("sinh cosh tanh asinh …", "iperboliche"),
    ("exp ln log(x) log(x,b) log2", "esponenziali e logaritmi"),
    ("sqrt cbrt root(x,n) |x|", "radici e valore assoluto"),
    ("floor ceil round trunc frac", "arrotondamenti"),
    ("n!  gamma  binom(n,k)  npr", "combinatoria"),
    ("min max mean gcd lcm mod", "varie"),
    ("if(cond, a, b)  < > ≤ ≥", "a tratti"),
    ("diff(f, x)  diff(f, x, x0)", "derivata numerica"),
    ("integral(f, x, a, b)", "integrale (anche ±inf)"),
    ("sum(f, k, a, b)  prod(…)", "sommatorie e produttorie"),
];

pub fn builtin_constant(name: &str) -> Option<f64> {
    Some(match name {
        "pi" => PI,
        "e" => E,
        "tau" => TAU,
        "phi" => (1.0 + 5f64.sqrt()) / 2.0,
        "inf" | "infinity" => f64::INFINITY,
        "nan" => f64::NAN,
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub enum Node {
    Const(f64),
    Slot(usize),
    Neg(Box<Node>),
    Fact(Box<Node>),
    Bin(BinOp, Box<Node>, Box<Node>),
    F1(fn(f64) -> f64, Box<Node>),
    F2(fn(f64, f64) -> f64, Box<Node>, Box<Node>),
    FN(fn(&[f64]) -> f64, Vec<Node>),
    If(Box<Node>, Box<Node>, Box<Node>),
    Diff { body: Box<Node>, slot: usize, at: Box<Node> },
    Integral { body: Box<Node>, slot: usize, a: Box<Node>, b: Box<Node> },
    Sum { body: Box<Node>, slot: usize, a: Box<Node>, b: Box<Node>, product: bool },
}

impl Node {
    pub fn eval(&self, s: &mut [f64]) -> f64 {
        match self {
            Node::Const(c) => *c,
            Node::Slot(i) => s[*i],
            Node::Neg(a) => -a.eval(s),
            Node::Fact(a) => factorial(a.eval(s)),
            Node::Bin(op, a, b) => {
                let (a, b) = (a.eval(s), b.eval(s));
                match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                    BinOp::Mod => a.rem_euclid(b),
                    BinOp::Pow => pow(a, b),
                    BinOp::Lt => bool_f(a < b),
                    BinOp::Gt => bool_f(a > b),
                    BinOp::Le => bool_f(a <= b),
                    BinOp::Ge => bool_f(a >= b),
                }
            }
            Node::F1(f, a) => f(a.eval(s)),
            Node::F2(f, a, b) => {
                let a = a.eval(s);
                f(a, b.eval(s))
            }
            Node::FN(f, args) => {
                let vals: Vec<f64> = args.iter().map(|a| a.eval(s)).collect();
                f(&vals)
            }
            Node::If(c, a, b) => {
                let c = c.eval(s);
                if c.is_nan() {
                    f64::NAN
                } else if c != 0.0 {
                    a.eval(s)
                } else {
                    b.eval(s)
                }
            }
            Node::Diff { body, slot, at } => {
                let x0 = at.eval(s);
                let saved = s[*slot];
                let h = 1e-3 * x0.abs().max(1.0);
                let mut f = |x: f64| {
                    s[*slot] = x;
                    body.eval(s)
                };
                let d = (-f(x0 + 2.0 * h) + 8.0 * f(x0 + h) - 8.0 * f(x0 - h) + f(x0 - 2.0 * h))
                    / (12.0 * h);
                s[*slot] = saved;
                d
            }
            Node::Integral { body, slot, a, b } => {
                let (a, b) = (a.eval(s), b.eval(s));
                let saved = s[*slot];
                let r = integrate(
                    |x| {
                        s[*slot] = x;
                        body.eval(s)
                    },
                    a,
                    b,
                );
                s[*slot] = saved;
                r
            }
            Node::Sum { body, slot, a, b, product } => {
                let (a, b) = (a.eval(s).round(), b.eval(s).round());
                if !a.is_finite() || !b.is_finite() || b - a > 1e6 {
                    return f64::NAN;
                }
                let saved = s[*slot];
                let mut acc = if *product { 1.0 } else { 0.0 };
                let mut k = a;
                while k <= b {
                    s[*slot] = k;
                    let v = body.eval(s);
                    if *product { acc *= v } else { acc += v }
                    k += 1.0;
                }
                s[*slot] = saved;
                acc
            }
        }
    }

}

fn bool_f(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// Potenza che gestisce radici dispari di numeri negativi: (-8)^(1/3) = -2.
fn pow(a: f64, b: f64) -> f64 {
    let r = a.powf(b);
    if r.is_nan() && a < 0.0 && b.is_finite() && b != 0.0 {
        let inv = 1.0 / b;
        let n = inv.round();
        if (inv - n).abs() < 1e-9 && n as i64 % 2 != 0 {
            return -(-a).powf(b);
        }
    }
    r
}

pub fn gamma(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == x.floor() && x <= 0.0 {
        return f64::NAN;
    }
    if x < 0.5 {
        return PI / ((PI * x).sin() * gamma(1.0 - x));
    }
    if x == x.floor() && x <= 171.0 {
        let mut r = 1.0;
        let mut k = 2.0;
        while k < x {
            r *= k;
            k += 1.0;
        }
        return r;
    }
    // Approssimazione di Lanczos (g = 7, n = 9)
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    let x = x - 1.0;
    let mut a = C[0];
    let t = x + G + 0.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    (2.0 * PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * a
}

fn factorial(x: f64) -> f64 {
    gamma(x + 1.0)
}

fn erf(x: f64) -> f64 {
    // Abramowitz & Stegun 7.1.26 raffinata con una serie per |x| piccoli
    if x.abs() < 2.0 {
        let mut sum = x;
        let mut term = x;
        let x2 = x * x;
        let mut n = 0.0;
        while term.abs() > 1e-17 * sum.abs() {
            n += 1.0;
            term *= -x2 / n;
            sum += term / (2.0 * n + 1.0);
        }
        return 2.0 / PI.sqrt() * sum;
    }
    let sign = x.signum();
    let x = x.abs();
    // Frazione continua per erfc a valori grandi
    let mut f = 0.0;
    for k in (1..60).rev() {
        f = (k as f64 / 2.0) / (x + f);
    }
    sign * (1.0 - (-x * x).exp() / PI.sqrt() / (x + f))
}

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.abs().round(), b.abs().round());
    if !a.is_finite() || !b.is_finite() {
        return f64::NAN;
    }
    while b > 0.0 {
        (a, b) = (b, a % b);
    }
    a
}

fn binom(n: f64, k: f64) -> f64 {
    if k < 0.0 || (n >= 0.0 && k > n) {
        return 0.0;
    }
    if n == n.round() && k == k.round() && n >= 0.0 {
        let k = k.min(n - k);
        let mut r = 1.0;
        let mut i = 0.0;
        while i < k {
            r = r * (n - i) / (i + 1.0);
            i += 1.0;
        }
        return r.round();
    }
    gamma(n + 1.0) / (gamma(k + 1.0) * gamma(n - k + 1.0))
}

/// Quadratura tanh-sinh (doppio esponenziale) su [a, b] finito.
fn tanh_sinh(mut f: impl FnMut(f64) -> f64, a: f64, b: f64) -> f64 {
    let c = 0.5 * (a + b);
    let d = 0.5 * (b - a);
    let half_pi = PI / 2.0;
    let mut sum = half_pi * f(c);
    let mut h = 1.0;
    let mut prev = f64::NAN;
    for level in 0..10 {
        let step = if level == 0 { 1 } else { 2 };
        let mut k = 1;
        loop {
            let t = k as f64 * h;
            let u = half_pi * t.sinh();
            let ch = u.cosh();
            let w = half_pi * t.cosh() / (ch * ch);
            // distanza dall'estremo, calcolata senza cancellazione
            let xc = d / (u.exp() * ch);
            if w * d.abs() < 1e-300 || xc.abs() < 1e-300 {
                break;
            }
            let (fl, fr) = (f(a + xc), f(b - xc));
            let fl = if fl.is_finite() { fl } else if xc.abs() < 1e-12 * d.abs() { 0.0 } else { fl };
            let fr = if fr.is_finite() { fr } else if xc.abs() < 1e-12 * d.abs() { 0.0 } else { fr };
            sum += w * (fl + fr);
            k += step;
            if t > 6.5 {
                break;
            }
        }
        let est = d * h * sum;
        if level > 3 && (est - prev).abs() <= 1e-12 * est.abs().max(1e-300) + 1e-14 {
            return est;
        }
        if !est.is_finite() && level > 1 {
            return est;
        }
        prev = est;
        h /= 2.0;
    }
    prev
}

pub fn integrate(mut f: impl FnMut(f64) -> f64, a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == b {
        return 0.0;
    }
    if a > b {
        return -integrate(f, b, a);
    }
    match (a.is_finite(), b.is_finite()) {
        (true, true) => tanh_sinh(f, a, b),
        // [a, ∞): x = a + t/(1-t), t ∈ [0, 1)
        (true, false) => tanh_sinh(
            |t| {
                let x = a + t / (1.0 - t);
                f(x) / ((1.0 - t) * (1.0 - t))
            },
            0.0,
            1.0,
        ),
        // (-∞, b]: x = b - t/(1-t)
        (false, true) => tanh_sinh(
            |t| {
                let x = b - t / (1.0 - t);
                f(x) / ((1.0 - t) * (1.0 - t))
            },
            0.0,
            1.0,
        ),
        // (-∞, ∞): x = t/(1-t²)
        (false, false) => tanh_sinh(
            |t| {
                let q = 1.0 - t * t;
                f(t / q) * (1.0 + t * t) / (q * q)
            },
            -1.0,
            1.0,
        ),
    }
}

fn trig_in(deg: bool, f: fn(f64) -> f64, fd: fn(f64) -> f64) -> fn(f64) -> f64 {
    if deg { fd } else { f }
}

fn builtin_f1(name: &str, deg: bool) -> Option<fn(f64) -> f64> {
    Some(match name {
        "sin" => trig_in(deg, f64::sin, |x| x.to_radians().sin()),
        "cos" => trig_in(deg, f64::cos, |x| x.to_radians().cos()),
        "tan" => trig_in(deg, f64::tan, |x| x.to_radians().tan()),
        "cot" => trig_in(deg, |x| 1.0 / x.tan(), |x| 1.0 / x.to_radians().tan()),
        "sec" => trig_in(deg, |x| 1.0 / x.cos(), |x| 1.0 / x.to_radians().cos()),
        "csc" => trig_in(deg, |x| 1.0 / x.sin(), |x| 1.0 / x.to_radians().sin()),
        "asin" | "arcsin" => trig_in(deg, f64::asin, |x| x.asin().to_degrees()),
        "acos" | "arccos" => trig_in(deg, f64::acos, |x| x.acos().to_degrees()),
        "atan" | "arctan" => trig_in(deg, f64::atan, |x| x.atan().to_degrees()),
        "sinh" => f64::sinh,
        "cosh" => f64::cosh,
        "tanh" => f64::tanh,
        "asinh" => f64::asinh,
        "acosh" => f64::acosh,
        "atanh" => f64::atanh,
        "exp" => f64::exp,
        "ln" => f64::ln,
        "log" | "log10" => f64::log10,
        "log2" => f64::log2,
        "sqrt" => f64::sqrt,
        "cbrt" => f64::cbrt,
        "abs" => f64::abs,
        "floor" => f64::floor,
        "ceil" => f64::ceil,
        "round" => f64::round,
        "trunc" => f64::trunc,
        "frac" => f64::fract,
        "sign" | "sgn" => |x| if x == 0.0 { 0.0 } else { x.signum() },
        "gamma" => gamma,
        "fact" => factorial,
        "erf" => erf,
        "erfc" => |x| 1.0 - erf(x),
        "deg" => f64::to_degrees,
        "rad" => f64::to_radians,
        _ => return None,
    })
}

fn builtin_f2(name: &str, deg: bool) -> Option<fn(f64, f64) -> f64> {
    Some(match name {
        "atan2" => {
            if deg {
                |y, x| f64::atan2(y, x).to_degrees()
            } else {
                f64::atan2
            }
        }
        "pow" => pow,
        "hypot" => f64::hypot,
        "mod" => f64::rem_euclid,
        "gcd" => gcd,
        "lcm" => |a, b| {
            let g = gcd(a, b);
            if g == 0.0 { 0.0 } else { (a * b).abs().round() / g }
        },
        "binom" | "ncr" => binom,
        "npr" => |n, k| gamma(n + 1.0) / gamma(n - k + 1.0),
        "root" => |x, n| pow(x, 1.0 / n),
        "log" => |x, b| x.ln() / b.ln(),
        _ => return None,
    })
}

#[derive(Clone)]
pub struct UserFunc {
    pub params: Vec<String>,
    pub body: Expr,
}

/// Ambiente globale: costanti definite dall'utente, funzioni, modalità angoli.
#[derive(Default, Clone)]
pub struct Env {
    pub consts: HashMap<String, f64>,
    pub funcs: HashMap<String, UserFunc>,
    pub degrees: bool,
}

enum Binding {
    Node(Node),
    Slot(usize),
}

/// Espressione compilata con le sue variabili libere (in ordine di comparsa).
#[derive(Clone, Debug)]
pub struct Compiled {
    pub root: Node,
    pub nslots: usize,
    pub free: Vec<(String, usize)>,
}

impl Compiled {
    pub fn free_names(&self) -> Vec<String> {
        self.free.iter().map(|(n, _)| n.clone()).collect()
    }

    pub fn slot_of(&self, name: &str) -> Option<usize> {
        self.free.iter().find(|(n, _)| n == name).map(|(_, s)| *s)
    }
}

struct Compiler<'a> {
    env: &'a Env,
    scope: Vec<(String, Binding)>,
    nslots: usize,
    free: Vec<(String, usize)>,
    depth: usize,
}

impl Compiler<'_> {
    fn new_slot(&mut self) -> usize {
        self.nslots += 1;
        self.nslots - 1
    }

    fn lookup(&mut self, name: &str) -> Node {
        if let Some((_, b)) = self.scope.iter().rev().find(|(n, _)| n == name) {
            return match b {
                Binding::Node(n) => n.clone(),
                Binding::Slot(s) => Node::Slot(*s),
            };
        }
        if let Some(v) = self.env.consts.get(name) {
            return Node::Const(*v);
        }
        if let Some(v) = builtin_constant(name) {
            return Node::Const(v);
        }
        if let Some((_, s)) = self.free.iter().find(|(n, _)| n == name) {
            return Node::Slot(*s);
        }
        let s = self.new_slot();
        self.free.push((name.to_string(), s));
        Node::Slot(s)
    }

    fn compile(&mut self, e: &Expr) -> Result<Node, String> {
        Ok(match e {
            Expr::Num(n) => Node::Const(*n),
            Expr::Var(name) => {
                if self.env.funcs.contains_key(name) || builtin_arity(name).is_some() {
                    return Err(format!("'{name}' è una funzione: usa {name}(…)"));
                }
                self.lookup(name)
            }
            Expr::Neg(a) => fold(Node::Neg(Box::new(self.compile(a)?))),
            Expr::Fact(a) => fold(Node::Fact(Box::new(self.compile(a)?))),
            Expr::Bin(op, a, b) => {
                let n = Node::Bin(*op, Box::new(self.compile(a)?), Box::new(self.compile(b)?));
                fold(n)
            }
            Expr::Tuple(_) => return Err("le coppie (a, b) sono ammesse solo come intera riga".into()),
            Expr::Call(name, args) => self.call(name, args)?,
        })
    }

    fn bound_var(&self, name: &str, args: &[Expr]) -> Result<String, String> {
        match &args[1] {
            Expr::Var(v) => Ok(v.clone()),
            _ => Err(format!("{name}: il secondo argomento deve essere una variabile")),
        }
    }

    fn with_bound<T>(
        &mut self,
        var: &str,
        f: impl FnOnce(&mut Self, usize) -> Result<T, String>,
    ) -> Result<T, String> {
        let slot = self.new_slot();
        self.scope.push((var.to_string(), Binding::Slot(slot)));
        let r = f(self, slot);
        self.scope.pop();
        r
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Result<Node, String> {
        if let Some(uf) = self.env.funcs.get(name) {
            if uf.params.len() != args.len() {
                return Err(format!(
                    "{name} richiede {} argomenti, ricevuti {}",
                    uf.params.len(),
                    args.len()
                ));
            }
            if self.depth >= MAX_INLINE_DEPTH {
                return Err(format!("{name}: ricorsione non supportata"));
            }
            let uf = uf.clone();
            let mut bound = Vec::with_capacity(args.len());
            for a in args {
                bound.push(self.compile(a)?);
            }
            // Il corpo vede solo i propri parametri (più le variabili globali).
            let saved = std::mem::take(&mut self.scope);
            self.scope = uf.params.iter().cloned().zip(bound.into_iter().map(Binding::Node)).collect();
            self.depth += 1;
            let r = self.compile(&uf.body);
            self.depth -= 1;
            self.scope = saved;
            return r;
        }
        let Some(arity) = builtin_arity(name) else {
            return Err(format!("funzione sconosciuta '{name}'"));
        };
        if !arity.accepts(args.len()) {
            return Err(format!("numero di argomenti errato per {name}"));
        }
        let deg = self.env.degrees;
        match name {
            "diff" | "derivative" => {
                let var = self.bound_var(name, args)?;
                // diff(f, x) senza punto: derivata nel valore corrente di x (che resta libera)
                let at = match args.get(2) {
                    Some(a) => self.compile(a)?,
                    None => self.lookup(&var),
                };
                return self.with_bound(&var, |c, slot| {
                    let body = Box::new(c.compile(&args[0])?);
                    Ok(fold(Node::Diff { body, slot, at: Box::new(at) }))
                });
            }
            "integral" | "integrate" | "int" | "sum" | "prod" => {
                let var = self.bound_var(name, args)?;
                let a = self.compile(&args[2])?;
                let b = self.compile(&args[3])?;
                return self.with_bound(&var, |c, slot| {
                    let body = Box::new(c.compile(&args[0])?);
                    let (a, b) = (Box::new(a), Box::new(b));
                    Ok(fold(if name == "sum" || name == "prod" {
                        Node::Sum { body, slot, a, b, product: name == "prod" }
                    } else {
                        Node::Integral { body, slot, a, b }
                    }))
                });
            }
            _ => {}
        }
        let mut nodes = Vec::with_capacity(args.len());
        for a in args {
            nodes.push(self.compile(a)?);
        }
        let node = match (name, nodes.len()) {
            ("if", _) => {
                let mut it = nodes.into_iter();
                let c = it.next().unwrap();
                let a = it.next().unwrap();
                let b = it.next().unwrap_or(Node::Const(f64::NAN));
                Node::If(Box::new(c), Box::new(a), Box::new(b))
            }
            ("min", _) => Node::FN(|v| v.iter().copied().fold(f64::INFINITY, f64::min), nodes),
            ("max", _) => Node::FN(|v| v.iter().copied().fold(f64::NEG_INFINITY, f64::max), nodes),
            ("mean", _) => Node::FN(|v| v.iter().sum::<f64>() / v.len() as f64, nodes),
            (_, 1) => {
                let f = builtin_f1(name, deg).ok_or_else(|| format!("{name}: argomenti errati"))?;
                Node::F1(f, Box::new(nodes.pop().unwrap()))
            }
            (_, 2) => {
                let f = builtin_f2(name, deg).ok_or_else(|| format!("{name}: argomenti errati"))?;
                let b = nodes.pop().unwrap();
                let a = nodes.pop().unwrap();
                Node::F2(f, Box::new(a), Box::new(b))
            }
            _ => return Err(format!("{name}: argomenti errati")),
        };
        Ok(fold(node))
    }
}

impl Node {
    /// Slot letti dal nodo che non sono vincolati al suo interno.
    fn open_slots(&self, out: &mut Vec<usize>) {
        let bound_scope = |slot: usize, inner: &[&Node], out: &mut Vec<usize>| {
            let mut v = Vec::new();
            for n in inner {
                n.open_slots(&mut v);
            }
            out.extend(v.into_iter().filter(|s| *s != slot));
        };
        match self {
            Node::Const(_) => {}
            Node::Slot(s) => out.push(*s),
            Node::Neg(a) | Node::Fact(a) | Node::F1(_, a) => a.open_slots(out),
            Node::Bin(_, a, b) | Node::F2(_, a, b) => {
                a.open_slots(out);
                b.open_slots(out);
            }
            Node::FN(_, args) => args.iter().for_each(|a| a.open_slots(out)),
            Node::If(c, a, b) => {
                c.open_slots(out);
                a.open_slots(out);
                b.open_slots(out);
            }
            Node::Diff { body, slot, at } => {
                at.open_slots(out);
                bound_scope(*slot, &[body], out);
            }
            Node::Integral { body, slot, a, b } | Node::Sum { body, slot, a, b, .. } => {
                a.open_slots(out);
                b.open_slots(out);
                bound_scope(*slot, &[body], out);
            }
        }
    }
}

/// Valuta subito i nodi che non dipendono da alcuno slot libero.
fn fold(n: Node) -> Node {
    if matches!(n, Node::Const(_) | Node::Slot(_)) {
        return n;
    }
    let mut open = Vec::new();
    n.open_slots(&mut open);
    if !open.is_empty() {
        return n;
    }
    // Gli slot vincolati interni vengono scritti durante la valutazione.
    let mut scratch = vec![0.0; max_slot(&n) + 1];
    Node::Const(n.eval(&mut scratch))
}

fn max_slot(n: &Node) -> usize {
    match n {
        Node::Const(_) => 0,
        Node::Slot(s) => *s,
        Node::Neg(a) | Node::Fact(a) | Node::F1(_, a) => max_slot(a),
        Node::Bin(_, a, b) | Node::F2(_, a, b) => max_slot(a).max(max_slot(b)),
        Node::FN(_, args) => args.iter().map(max_slot).max().unwrap_or(0),
        Node::If(c, a, b) => max_slot(c).max(max_slot(a)).max(max_slot(b)),
        Node::Diff { body, slot, at } => (*slot).max(max_slot(body)).max(max_slot(at)),
        Node::Integral { body, slot, a, b } | Node::Sum { body, slot, a, b, .. } => {
            (*slot).max(max_slot(body)).max(max_slot(a)).max(max_slot(b))
        }
    }
}

pub fn compile(expr: &Expr, env: &Env) -> Result<Compiled, String> {
    let mut c = Compiler { env, scope: Vec::new(), nslots: 0, free: Vec::new(), depth: 0 };
    let root = c.compile(expr)?;
    Ok(Compiled { root, nslots: c.nslots, free: c.free })
}

/// Formattazione "da calcolatrice": 12 cifre significative, notazione scientifica
/// solo quando serve, niente zeri finali.
pub fn format_number(v: f64) -> String {
    if v.is_nan() {
        return "non definito".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "∞".into() } else { "−∞".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    let s = if !(1e-6..1e15).contains(&a) {
        let s = format!("{v:.10e}");
        let (mant, exp) = s.split_once('e').unwrap();
        let mant = trim_zeros(mant);
        let exp: i32 = exp.parse().unwrap();
        format!("{mant} × 10{}", superscript(exp))
    } else {
        let digits = 12 - (a.log10().floor() as i32 + 1);
        let s = format!("{v:.*}", digits.clamp(0, 15) as usize);
        trim_zeros(&s).to_string()
    };
    s.replace('-', "−")
}

fn trim_zeros(s: &str) -> &str {
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s }
}

fn superscript(n: i32) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}

/// Versione compatta per gli slider e le etichette.
pub fn format_short(v: f64) -> String {
    if v.is_finite() && v.abs() >= 1e-4 && v.abs() < 1e6 {
        let s = format!("{v:.4}");
        trim_zeros(&s).replace('-', "−")
    } else {
        format_number(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Statement, parse_statement};
    use std::collections::HashSet;

    fn eval(src: &str) -> f64 {
        let Statement::Expr(e) = parse_statement(src, &HashSet::new()).unwrap() else { panic!() };
        let c = compile(&e, &Env::default()).unwrap();
        assert!(c.free.is_empty(), "variabili libere in {src}");
        c.root.eval(&mut vec![0.0; c.nslots])
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-8 * b.abs().max(1.0)
    }

    #[test]
    fn arithmetic() {
        assert_eq!(eval("1 + 2 * 3"), 7.0);
        assert_eq!(eval("2^3^2"), 512.0);
        assert_eq!(eval("-2^2"), -4.0);
        assert_eq!(eval("10 % 3"), 1.0);
        assert_eq!(eval("100 - 10%"), 90.0);
        assert_eq!(eval("50 + 20%"), 60.0);
        assert_eq!(eval("50 * 10%"), 5.0);
        assert_eq!(eval("10%"), 0.1);
        assert_eq!(eval("100 - 10% - 10%"), 81.0);
        assert_eq!(eval("(1 + 2)%"), 0.03);
        // Testi inseriti dai tasti del tastierino scientifico
        assert_eq!(eval("2×10^3"), 2000.0);
        assert_eq!(eval("log2(8) + root(8, 3) + cbrt(27) + sign(−2)"), 7.0);
        assert_eq!(eval("binom(5, 2) + gamma(4) + 1/(4) + abs(−1)"), 17.25);
        assert!((eval("τ − 2π") + eval("φ − 1/φ") - 1.0).abs() < 1e-12);
        assert!((eval("asinh(sinh(1)) + 10^log(3) + 2^log2(5)") - 9.0).abs() < 1e-12);
        assert_eq!(eval("5!"), 120.0);
        assert_eq!(eval("2(3+4)"), 14.0);
        assert_eq!(eval("(-8)^(1/3)"), -2.0);
        assert!(close(eval("2pi"), TAU));
        assert!(close(eval("sin(pi/2)"), 1.0));
        assert!(close(eval("√16 + |−3|"), 7.0));
        assert!(close(eval("log(8, 2)"), 3.0));
        assert!(close(eval("gamma(0.5)"), PI.sqrt()));
        assert_eq!(eval("binom(10, 3)"), 120.0);
        assert!(close(eval("erf(1)"), 0.842_700_792_949_714_9));
        assert!(close(eval("erf(3)"), 0.999_977_909_503_001_4));
    }

    #[test]
    fn calculus() {
        assert!(close(eval("integral(x^2, x, 0, 3)"), 9.0));
        assert!(close(eval("integral(exp(-x^2), x, -inf, inf)"), PI.sqrt()));
        assert!(close(eval("integral(1/sqrt(x), x, 0, 1)"), 2.0));
        assert!(close(eval("integral(exp(-x), x, 0, inf)"), 1.0));
        assert!(close(eval("diff(x^3, x, 2)"), 12.0));
        assert_eq!(eval("sum(k, k, 1, 100)"), 5050.0);
        assert_eq!(eval("prod(k, k, 1, 5)"), 120.0);
    }

    #[test]
    fn free_variables() {
        let Statement::Expr(e) = parse_statement("a*sin(x) + integral(t, t, 0, x)", &HashSet::new()).unwrap()
        else {
            panic!()
        };
        let c = compile(&e, &Env::default()).unwrap();
        assert_eq!(c.free_names(), vec!["a", "x"]);
    }

    #[test]
    fn formatting() {
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(-2.5), "−2.5");
        assert_eq!(format_number(1e20), "1 × 10²⁰");
        assert_eq!(format_number(1.0 / 3.0), "0.333333333333");
    }
}
