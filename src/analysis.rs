//! Analisi del "quaderno" di espressioni: risolve definizioni, sceglie gli assi
//! del grafico e trasforma in parametri (slider) le variabili rimaste libere.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::eval::{Compiled, Env, UserFunc, compile};
use crate::parser::{BinOp, Expr, Statement, parse_statement, peek_func_def};

/// Variabili che fungono da assi: non diventano mai slider.
const AXIS_VARS: [&str; 4] = ["x", "y", "t", "theta"];

/// Un'espressione pronta da campionare: slot già allocati, assi da riempire.
#[derive(Clone)]
pub struct Func {
    c: Compiled,
    slots: Vec<f64>,
    a: Option<usize>,
    b: Option<usize>,
}

impl Func {
    fn new(c: Compiled, a: &str, b: &str) -> Self {
        let (sa, sb) = (c.slot_of(a), c.slot_of(b));
        Func { slots: vec![0.0; c.nslots], c, a: sa, b: sb }
    }

    pub fn eval1(&mut self, a: f64) -> f64 {
        if let Some(s) = self.a {
            self.slots[s] = a;
        }
        self.c.root.eval(&mut self.slots)
    }

    pub fn eval2(&mut self, a: f64, b: f64) -> f64 {
        if let Some(s) = self.b {
            self.slots[s] = b;
        }
        self.eval1(a)
    }
}

#[derive(Clone)]
pub enum Plot {
    None,
    /// y = f(var), con `var` sull'asse orizzontale
    Function { f: Func },
    /// x = g(y)
    FunctionOfY { g: Func },
    /// Mappa di colore di f(x, y)
    Field { f: Func },
    /// Curva implicita f(x, y) = 0
    Implicit { f: Func },
    /// Soluzioni di f(var) = 0, disegnate come rette verticali (o orizzontali se var = y)
    Roots { f: Func, var: String, horizontal: bool },
    Parametric { fx: Func, fy: Func },
    Polar { r: Func },
    Point { x: f64, y: f64 },
}

impl Plot {
    pub fn is_some(&self) -> bool {
        !matches!(self, Plot::None)
    }
}

#[derive(Clone)]
pub enum Summary {
    Empty,
    Value(f64),
    Bool { ok: bool, lhs: f64, rhs: f64 },
    Defined(f64),
    Function(String),
    Info(String),
    Error(String),
}

#[derive(Clone)]
pub struct Analysis {
    pub summary: Summary,
    pub plot: Plot,
    /// Variabile sull'asse orizzontale (per le etichette)
    pub axis: Option<String>,
    /// Alternative selezionabili per l'asse
    pub axis_choices: Vec<String>,
    /// Parametri (slider) usati da questa riga
    pub params: Vec<String>,
}

impl Analysis {
    fn summary(summary: Summary) -> Self {
        Analysis { summary, plot: Plot::None, axis: None, axis_choices: Vec::new(), params: Vec::new() }
    }

    fn error(msg: impl Into<String>) -> Self {
        Self::summary(Summary::Error(msg.into()))
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Param {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    #[serde(skip)]
    pub playing: bool,
    #[serde(skip, default = "one")]
    pub direction: f64,
}

fn one() -> f64 {
    1.0
}

impl Default for Param {
    fn default() -> Self {
        Param { value: 1.0, min: -10.0, max: 10.0, playing: false, direction: 1.0 }
    }
}

pub struct Input<'a> {
    pub text: &'a str,
    pub axis_override: Option<&'a str>,
}

pub struct Notebook {
    pub rows: Vec<Analysis>,
    /// Parametri in uso, nell'ordine di comparsa
    pub params: Vec<String>,
}

enum Kind {
    Expr(Expr),
    Tuple(Expr, Expr),
    Equation(Expr, Expr),
    Def(String, Expr),
    FuncDef(String, Vec<String>),
}

fn is_reserved(name: &str) -> bool {
    matches!(name, "x" | "y")
}

/// Sceglie la variabile per l'asse orizzontale tra quelle libere.
fn pick_axis(free: &[String], over: Option<&str>) -> Option<String> {
    if let Some(o) = over
        && free.iter().any(|f| f == o)
    {
        return Some(o.to_string());
    }
    for pref in ["x", "t", "theta", "y"] {
        if free.iter().any(|f| f == pref) {
            return Some(pref.to_string());
        }
    }
    free.first().cloned()
}

fn contains_var(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Var(v) => v == name,
        Expr::Num(_) => false,
        Expr::Neg(a) | Expr::Fact(a) => contains_var(a, name),
        Expr::Bin(_, a, b) => contains_var(a, name) || contains_var(b, name),
        Expr::Call(_, args) | Expr::Tuple(args) => args.iter().any(|a| contains_var(a, name)),
    }
}

fn diff_expr(lhs: &Expr, rhs: &Expr) -> Expr {
    Expr::Bin(BinOp::Sub, Box::new(lhs.clone()), Box::new(rhs.clone()))
}

fn free_of(e: &Expr, env: &Env) -> Result<Vec<String>, String> {
    compile(e, env).map(|c| c.free_names())
}

pub fn analyze(inputs: &[Input], params: &mut BTreeMap<String, Param>, degrees: bool) -> Notebook {
    let mut env = Env { degrees, ..Default::default() };

    // 1. Nomi delle funzioni utente, poi parsing di tutte le righe.
    let func_names: HashSet<String> = inputs.iter().filter_map(|i| peek_func_def(i.text)).collect();
    let mut kinds: Vec<Result<Option<Kind>, String>> = Vec::with_capacity(inputs.len());
    for input in inputs {
        if input.text.trim().is_empty() {
            kinds.push(Ok(None));
            continue;
        }
        let k = parse_statement(input.text, &func_names).and_then(|stmt| {
            Ok(Some(match stmt {
                Statement::Expr(Expr::Tuple(items)) => {
                    let [a, b]: [Expr; 2] =
                        items.try_into().map_err(|_| "un punto ha due coordinate: (x, y)".to_string())?;
                    Kind::Tuple(a, b)
                }
                Statement::Expr(e) => Kind::Expr(e),
                Statement::Equation(l, r) => Kind::Equation(l, r),
                Statement::Assign(name, rhs) => {
                    if is_reserved(&name) || (name == "r" && contains_var(&rhs, "theta")) {
                        Kind::Equation(Expr::Var(name), rhs)
                    } else if crate::eval::builtin_constant(&name).is_some() {
                        return Err(format!("'{name}' è una costante predefinita"));
                    } else {
                        Kind::Def(name, rhs)
                    }
                }
                Statement::FuncDef(name, ps, body) => {
                    if env.funcs.contains_key(&name) {
                        return Err(format!("{name} è già definita"));
                    }
                    env.funcs.insert(name.clone(), UserFunc { params: ps.clone(), body });
                    Kind::FuncDef(name, ps)
                }
            }))
        });
        kinds.push(k);
    }

    // 2. Definizioni di variabili, risolte per punto fisso.
    let def_names: HashSet<String> = kinds
        .iter()
        .filter_map(|k| match k {
            Ok(Some(Kind::Def(n, _))) => Some(n.clone()),
            _ => None,
        })
        .collect();
    let mut def_values: HashMap<usize, Result<f64, String>> = HashMap::new();
    let mut defined_by: HashMap<String, usize> = HashMap::new();
    let resolve_defs =
        |env: &mut Env, def_values: &mut HashMap<usize, Result<f64, String>>, defined_by: &mut HashMap<String, usize>| loop {
            let mut progress = false;
            for (i, k) in kinds.iter().enumerate() {
                let Ok(Some(Kind::Def(name, rhs))) = k else { continue };
                if def_values.contains_key(&i) {
                    continue;
                }
                if let Some(&j) = defined_by.get(name) {
                    if j != i {
                        def_values.insert(i, Err(format!("{name} è già definita")));
                    }
                    continue;
                }
                match compile(rhs, env) {
                    Ok(c) if c.free.is_empty() => {
                        let v = c.root.eval(&mut vec![0.0; c.nslots]);
                        env.consts.insert(name.clone(), v);
                        defined_by.insert(name.clone(), i);
                        def_values.insert(i, Ok(v));
                        progress = true;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        def_values.insert(i, Err(e));
                    }
                }
            }
            if !progress {
                break;
            }
        };
    resolve_defs(&mut env, &mut def_values, &mut defined_by);

    // 3. Variabili libere rimaste → parametri con slider.
    let mut used_params: Vec<String> = Vec::new();
    let add_param = |name: &str, used: &mut Vec<String>| {
        if AXIS_VARS.contains(&name) || def_names.contains(name) || used.iter().any(|p| p == name) {
            return;
        }
        used.push(name.to_string());
    };
    for (i, k) in kinds.iter().enumerate() {
        let over = inputs[i].axis_override;
        let candidates: Vec<String> = match k {
            Ok(Some(Kind::Expr(e))) => analyze_expr(e, &env, over).params,
            Ok(Some(Kind::Tuple(x, y))) => {
                // Senza t, tutte le variabili libere di un punto diventano slider.
                let a = analyze_tuple(x, y, &env, over);
                if matches!(a.plot, Plot::Parametric { .. }) {
                    a.params
                } else {
                    let mut f = free_of(x, &env).unwrap_or_default();
                    f.extend(free_of(y, &env).unwrap_or_default());
                    f
                }
            }
            Ok(Some(Kind::Equation(l, r))) => analyze_equation(l, r, &env, over).params,
            Ok(Some(Kind::Def(_, rhs))) if !def_values.contains_key(&i) => {
                let f = free_of(rhs, &env).unwrap_or_default();
                if f.iter().any(|v| AXIS_VARS.contains(&v.as_str())) {
                    analyze_expr(rhs, &env, over).params
                } else {
                    f
                }
            }
            Ok(Some(Kind::FuncDef(name, ps))) if ps.len() <= 2 => {
                analyze_expr(&func_plot_call(name, ps), &env, None).params
            }
            _ => Vec::new(),
        };
        for c in candidates {
            add_param(&c, &mut used_params);
        }
    }
    for p in &used_params {
        let v = params.entry(p.clone()).or_default().value;
        env.consts.insert(p.clone(), v);
    }
    resolve_defs(&mut env, &mut def_values, &mut defined_by);

    // 4. Analisi finale di ogni riga con l'ambiente completo.
    let rows = kinds
        .into_iter()
        .enumerate()
        .map(|(i, k)| {
            let over = inputs[i].axis_override;
            let mut a = match k {
                Err(e) => Analysis::error(e),
                Ok(None) => Analysis::summary(Summary::Empty),
                Ok(Some(Kind::Expr(e))) => analyze_expr(&e, &env, over),
                Ok(Some(Kind::Tuple(x, y))) => analyze_tuple(&x, &y, &env, over),
                Ok(Some(Kind::Equation(l, r))) => analyze_equation(&l, &r, &env, over),
                Ok(Some(Kind::Def(name, rhs))) => match def_values.get(&i) {
                    Some(Ok(v)) => Analysis::summary(Summary::Defined(*v)),
                    Some(Err(e)) => Analysis::error(e.clone()),
                    None => {
                        let f = free_of(&rhs, &env).unwrap_or_default();
                        if f.iter().any(|v| AXIS_VARS.contains(&v.as_str())) {
                            let mut a = analyze_expr(&rhs, &env, over);
                            if a.plot.is_some() {
                                a.summary = Summary::Info(format!(
                                    "grafico di {name} (dipende da {})",
                                    a.axis.clone().unwrap_or_default()
                                ));
                            }
                            a
                        } else {
                            Analysis::error(format!("definizione circolare di {name}"))
                        }
                    }
                },
                Ok(Some(Kind::FuncDef(name, ps))) => {
                    let sig = format!("{name}({})", ps.join(", "));
                    let mut a = if ps.len() <= 2 {
                        analyze_expr(&func_plot_call(&name, &ps), &env, None)
                    } else {
                        Analysis::summary(Summary::Empty)
                    };
                    a.axis_choices.clear();
                    a.summary = match a.summary {
                        Summary::Error(e) => Summary::Error(e),
                        _ => Summary::Function(sig),
                    };
                    a
                }
            };
            a.params.retain(|p| used_params.contains(p));
            a
        })
        .collect();

    Notebook { rows, params: used_params }
}

/// `f(x)` o `f(x, y)` per disegnare una funzione definita dall'utente.
fn func_plot_call(name: &str, ps: &[String]) -> Expr {
    let vars = ["x", "y"];
    Expr::Call(name.to_string(), ps.iter().zip(vars).map(|(_, v)| Expr::Var(v.into())).collect())
}

fn tuple_axis(free: &[String], over: Option<&str>) -> Option<String> {
    if let Some(o) = over
        && free.iter().any(|f| f == o)
    {
        return Some(o.to_string());
    }
    ["t", "theta"].into_iter().find(|p| free.iter().any(|f| f == p)).map(String::from)
}

fn params_of(free: &[String], axes: &[&str]) -> Vec<String> {
    free.iter().filter(|f| !axes.contains(&f.as_str())).cloned().collect()
}

fn analyze_expr(e: &Expr, env: &Env, over: Option<&str>) -> Analysis {
    let c = match compile(e, env) {
        Ok(c) => c,
        Err(err) => return Analysis::error(err),
    };
    let free = c.free_names();
    if free.is_empty() {
        let v = c.root.eval(&mut vec![0.0; c.nslots]);
        return Analysis::summary(Summary::Value(v));
    }
    let has = |n: &str| free.iter().any(|f| f == n);
    if has("x") && has("y") {
        return Analysis {
            summary: Summary::Info("mappa di colore f(x, y)".into()),
            params: params_of(&free, &["x", "y"]),
            plot: Plot::Field { f: Func::new(c, "x", "y") },
            axis: Some("x".into()),
            axis_choices: Vec::new(),
        };
    }
    let axis = pick_axis(&free, over).unwrap();
    Analysis {
        summary: Summary::Info(format!("funzione di {}", pretty(&axis))),
        params: params_of(&free, &[&axis]),
        plot: Plot::Function { f: Func::new(c, &axis, "") },
        axis_choices: if free.len() > 1 { free.clone() } else { Vec::new() },
        axis: Some(axis),
    }
}

fn analyze_tuple(x: &Expr, y: &Expr, env: &Env, over: Option<&str>) -> Analysis {
    let (cx, cy) = match (compile(x, env), compile(y, env)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return Analysis::error(e),
    };
    let mut free = cx.free_names();
    for n in cy.free_names() {
        if !free.contains(&n) {
            free.push(n);
        }
    }
    match tuple_axis(&free, over) {
        None => {
            if !free.is_empty() {
                return Analysis::error(format!("variabile non definita: {}", free.join(", ")));
            }
            let px = cx.root.eval(&mut vec![0.0; cx.nslots]);
            let py = cy.root.eval(&mut vec![0.0; cy.nslots]);
            Analysis {
                summary: Summary::Info(format!(
                    "punto ({}, {})",
                    crate::eval::format_short(px),
                    crate::eval::format_short(py)
                )),
                plot: Plot::Point { x: px, y: py },
                axis: None,
                axis_choices: Vec::new(),
                params: Vec::new(),
            }
        }
        Some(t) => Analysis {
            summary: Summary::Info(format!("curva parametrica in {}", pretty(&t))),
            params: params_of(&free, &[&t]),
            plot: Plot::Parametric { fx: Func::new(cx, &t, ""), fy: Func::new(cy, &t, "") },
            axis_choices: if free.len() > 1 { free.clone() } else { Vec::new() },
            axis: Some(t),
        },
    }
}

fn analyze_equation(l: &Expr, r: &Expr, env: &Env, over: Option<&str>) -> Analysis {
    let d = diff_expr(l, r);
    let c = match compile(&d, env) {
        Ok(c) => c,
        Err(e) => return Analysis::error(e),
    };
    let free = c.free_names();
    let has = |n: &str| free.iter().any(|f| f == n);
    let lhs_is = |n: &str| matches!(l, Expr::Var(v) if v == n);

    if free.is_empty() {
        let lc = compile(l, env).map(|c| c.root.eval(&mut vec![0.0; c.nslots]));
        let rc = compile(r, env).map(|c| c.root.eval(&mut vec![0.0; c.nslots]));
        let (Ok(lhs), Ok(rhs)) = (lc, rc) else { return Analysis::error("equazione non valida") };
        let ok = (lhs - rhs).abs() <= 1e-12 * lhs.abs().max(rhs.abs()).max(1.0);
        return Analysis::summary(Summary::Bool { ok, lhs, rhs });
    }

    // r = f(θ): curva polare
    if lhs_is("r") && has("theta") && !contains_var(r, "r") {
        let Ok(cr) = compile(r, env) else { return Analysis::error("espressione non valida") };
        let free = cr.free_names();
        return Analysis {
            summary: Summary::Info("curva polare r(θ)".into()),
            params: params_of(&free, &["theta"]),
            plot: Plot::Polar { r: Func::new(cr, "theta", "") },
            axis: Some("theta".into()),
            axis_choices: Vec::new(),
        };
    }

    if has("x") && has("y") {
        let params = params_of(&free, &["x", "y"]);
        // y = f(x) e x = g(y) esplicite: campionamento diretto, più preciso
        if lhs_is("y") && !contains_var(r, "y") {
            let cr = compile(r, env).unwrap();
            return Analysis {
                summary: Summary::Info("y = f(x)".into()),
                params,
                plot: Plot::Function { f: Func::new(cr, "x", "") },
                axis: Some("x".into()),
                axis_choices: Vec::new(),
            };
        }
        if lhs_is("x") && !contains_var(r, "x") {
            let cr = compile(r, env).unwrap();
            return Analysis {
                summary: Summary::Info("x = g(y)".into()),
                params,
                plot: Plot::FunctionOfY { g: Func::new(cr, "y", "") },
                axis: Some("y".into()),
                axis_choices: Vec::new(),
            };
        }
        return Analysis {
            summary: Summary::Info("curva implicita".into()),
            params,
            plot: Plot::Implicit { f: Func::new(c, "x", "y") },
            axis: Some("x".into()),
            axis_choices: Vec::new(),
        };
    }

    // y = costante (o con parametri): retta orizzontale
    if lhs_is("y") && !has("x") && !contains_var(r, "y") {
        let cr = compile(r, env).unwrap();
        let free_r = cr.free_names();
        if free_r.is_empty() {
            return Analysis {
                summary: Summary::Info("retta orizzontale".into()),
                params: Vec::new(),
                plot: Plot::Function { f: Func::new(cr, "x", "") },
                axis: Some("x".into()),
                axis_choices: Vec::new(),
            };
        }
    }

    let var = pick_axis(&free, over).unwrap();
    Analysis {
        summary: Summary::Info(format!("soluzioni in {}", pretty(&var))),
        params: params_of(&free, &[&var]),
        plot: Plot::Roots { f: Func::new(c, &var, ""), horizontal: var == "y", var: var.clone() },
        axis_choices: if free.len() > 1 { free.clone() } else { Vec::new() },
        axis: Some(var),
    }
}

pub fn pretty(name: &str) -> String {
    match name {
        "theta" => "θ".into(),
        "pi" => "π".into(),
        "tau" => "τ".into(),
        "phi" => "φ".into(),
        _ => name.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lines: &[&str]) -> (Notebook, BTreeMap<String, Param>) {
        let inputs: Vec<Input> = lines.iter().map(|t| Input { text: t, axis_override: None }).collect();
        let mut params = BTreeMap::new();
        let nb = analyze(&inputs, &mut params, false);
        (nb, params)
    }

    #[test]
    fn definitions_and_values() {
        let (nb, _) = run(&["b = a + 1", "a = 2", "f(x) = x^2 + b", "f(3)"]);
        assert!(matches!(nb.rows[0].summary, Summary::Defined(v) if v == 3.0));
        assert!(matches!(nb.rows[3].summary, Summary::Value(v) if v == 12.0));
        assert!(nb.params.is_empty());
    }

    #[test]
    fn sliders_for_free_variables() {
        let (nb, params) = run(&["a sin(b x)", "c = k * 2"]);
        assert_eq!(nb.params, vec!["a", "b", "k"]);
        assert!(params.contains_key("k"));
        assert!(matches!(nb.rows[0].plot, Plot::Function { .. }));
        assert!(matches!(nb.rows[1].summary, Summary::Defined(v) if v == 2.0));
    }

    #[test]
    fn plot_kinds() {
        let (nb, _) = run(&[
            "x^2 + y^2 = 1",
            "y = 2x",
            "x^2 = 2",
            "(cos t, sin t)",
            "x*y",
            "r = 1 + cos(theta)",
            "(1, 2)",
            "sin(z)",
        ]);
        assert!(matches!(nb.rows[0].plot, Plot::Implicit { .. }));
        assert!(matches!(nb.rows[1].plot, Plot::Function { .. }));
        assert!(matches!(nb.rows[2].plot, Plot::Roots { .. }));
        assert!(matches!(nb.rows[3].plot, Plot::Parametric { .. }));
        assert!(matches!(nb.rows[4].plot, Plot::Field { .. }));
        assert!(matches!(nb.rows[5].plot, Plot::Polar { .. }));
        assert!(matches!(nb.rows[6].plot, Plot::Point { x: 1.0, y: 2.0 }));
        assert!(matches!(nb.rows[7].plot, Plot::Function { .. }));
        assert!(nb.params.is_empty());
    }

    #[test]
    fn equation_check_and_errors() {
        let (nb, _) = run(&["2 + 2 = 4", "a = 1", "a = 2", "p = q", "q = p"]);
        assert!(matches!(nb.rows[0].summary, Summary::Bool { ok: true, .. }));
        assert!(matches!(nb.rows[2].summary, Summary::Error(_)));
        assert!(matches!(nb.rows[3].summary, Summary::Error(_)));
    }

    #[test]
    fn readme_examples() {
        let (nb, _) = run(&[
            "sin(x)", "y = x²", "x = y²", "x² + y² = 16", "sin(x) cos(y)", "(cos t, sin 2t)",
            "r = 1 + cos θ", "x³ − 2x = 1", "a sin(k x)",
            "100 - 10%", "f(x) = x² + 3", "f(2)", "integral(exp(-t^2), t, -inf, inf)",
            "sum(1/k^2, k, 1, 100)", "if(x < 0, -x, x)", "2x + 3(x+1) + sin x + √2 + |x| + 5! + π",
        ]);
        let p = |i: usize| &nb.rows[i].plot;
        assert!(matches!(p(0), Plot::Function { .. }));
        assert!(matches!(p(1), Plot::Function { .. }));
        assert!(matches!(p(2), Plot::FunctionOfY { .. }));
        assert!(matches!(p(3), Plot::Implicit { .. }));
        assert!(matches!(p(4), Plot::Field { .. }));
        assert!(matches!(p(5), Plot::Parametric { .. }));
        assert!(matches!(p(6), Plot::Polar { .. }));
        assert!(matches!(p(7), Plot::Roots { .. }));
        assert!(matches!(p(8), Plot::Function { .. }));
        assert_eq!(nb.params, vec!["a", "k"]);
        assert!(matches!(nb.rows[9].summary, Summary::Value(v) if v == 90.0));
        assert!(matches!(nb.rows[11].summary, Summary::Value(v) if v == 7.0));
        assert!(matches!(nb.rows[12].summary, Summary::Value(v) if (v - std::f64::consts::PI.sqrt()).abs() < 1e-9));
        assert!(matches!(nb.rows[13].summary, Summary::Value(v) if (v - 1.6349839).abs() < 1e-6));
        assert!(matches!(p(14), Plot::Function { .. }));
        assert!(matches!(p(15), Plot::Function { .. }));
    }
}
