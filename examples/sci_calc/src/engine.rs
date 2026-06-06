//! \file
//! \brief Pure algebraic-immediate-execution calculator engine.
//!
//! Host-free so it can be unit-tested on the build host. All display/I/O lives
//! in `lib.rs`. Transcendental math uses the `libm` crate (the SDK ships no
//! libm). Numbers are kept as the literal entry string while typing and parsed
//! to `f64` on demand, so backspace and decimals are exact.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::f64::consts::{E, PI};

const MAX_ENTRY_LEN: usize = 15;
const MAX_HISTORY: usize = 200;

/// \brief Angle interpretation for trigonometric functions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AngleMode {
    Deg,
    Rad,
}

/// \brief Error states surfaced on the display until cleared.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalcError {
    DivZero,
    Domain,
    Overflow,
}

/// \brief Binary operators applied left-to-right (no precedence).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Mod,
    Percent,
}

impl Op {
    fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "x",
            Op::Div => "/",
            Op::Pow => "^",
            Op::Mod => "mod",
            Op::Percent => "%",
        }
    }

    fn apply(self, a: f64, b: f64) -> Result<f64, CalcError> {
        let r = match self {
            Op::Add => a + b,
            Op::Sub => a - b,
            Op::Mul => a * b,
            Op::Div => {
                if b == 0.0 {
                    return Err(CalcError::DivZero);
                }
                a / b
            }
            Op::Pow => libm::pow(a, b),
            Op::Mod => {
                if b == 0.0 {
                    return Err(CalcError::DivZero);
                }
                libm::fmod(a, b)
            }
            Op::Percent => a * b / 100.0,
        };
        finite(r)
    }
}

/// \brief Unary functions applied immediately to the current value.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Unary {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Ln,
    Log10,
    Exp,
    Exp10,
    Sqr,
    Sqrt,
    Cbrt,
    Recip,
    Fact,
}

impl Unary {
    fn name(self) -> &'static str {
        match self {
            Unary::Sin => "sin",
            Unary::Cos => "cos",
            Unary::Tan => "tan",
            Unary::Asin => "asin",
            Unary::Acos => "acos",
            Unary::Atan => "atan",
            Unary::Ln => "ln",
            Unary::Log10 => "log",
            Unary::Exp => "exp",
            Unary::Exp10 => "10^",
            Unary::Sqr => "sqr",
            Unary::Sqrt => "sqrt",
            Unary::Cbrt => "cbrt",
            Unary::Recip => "1/",
            Unary::Fact => "fact",
        }
    }
}

fn finite(x: f64) -> Result<f64, CalcError> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(CalcError::Overflow)
    }
}

fn factorial(x: f64) -> Result<f64, CalcError> {
    if x < 0.0 || libm::floor(x) != x {
        return Err(CalcError::Domain);
    }
    if x > 170.0 {
        return Err(CalcError::Overflow);
    }
    let n = x as u64;
    let mut r = 1.0f64;
    let mut i = 2u64;
    while i <= n {
        r *= i as f64;
        i += 1;
    }
    finite(r)
}

/// \brief Format an `f64` for the display: trims trailing zeros, switches to
///        scientific notation for very large/small magnitudes, hides float
///        noise by rounding to ~10 fractional digits.
pub fn format_num(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string();
    }
    if !x.is_finite() {
        return "inf".to_string();
    }
    let a = libm::fabs(x);
    if a >= 1e16 || a < 1e-9 {
        return format_sci(x);
    }
    let mut s = format!("{:.10}", x);
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

fn format_sci(x: f64) -> String {
    let neg = x < 0.0;
    let a = libm::fabs(x);
    let mut exp = libm::floor(libm::log10(a)) as i32;
    let mut mant = a / libm::pow(10.0, exp as f64);
    if mant >= 10.0 {
        mant /= 10.0;
        exp += 1;
    } else if mant < 1.0 {
        mant *= 10.0;
        exp -= 1;
    }
    let mut m = format!("{:.6}", mant);
    if m.contains('.') {
        while m.ends_with('0') {
            m.pop();
        }
        if m.ends_with('.') {
            m.pop();
        }
    }
    format!("{}{}e{}", if neg { "-" } else { "" }, m, exp)
}

/// \brief Algebraic immediate-execution calculator state machine.
pub struct Calc {
    entry: String,
    acc: f64,
    pending: Option<Op>,
    last_binary: Option<(Op, f64)>,
    new_entry: bool,
    error: Option<CalcError>,
    pub angle: AngleMode,
    memory: f64,
    pub mem_set: bool,
    pub history: Vec<String>,
}

impl Calc {
    pub const fn new() -> Self {
        Self {
            entry: String::new(),
            acc: 0.0,
            pending: None,
            last_binary: None,
            new_entry: true,
            error: None,
            angle: AngleMode::Deg,
            memory: 0.0,
            mem_set: false,
            history: Vec::new(),
        }
    }

    pub fn error(&self) -> Option<CalcError> {
        self.error
    }

    pub fn pending_symbol(&self) -> &'static str {
        self.pending.map(|o| o.symbol()).unwrap_or("")
    }

    /// \brief The string currently shown on the top (input/output) line.
    pub fn display(&self) -> &str {
        if self.entry.is_empty() {
            "0"
        } else {
            &self.entry
        }
    }

    fn value(&self) -> f64 {
        self.entry.parse::<f64>().unwrap_or(0.0)
    }

    fn set_result(&mut self, v: f64) {
        self.entry = format_num(v);
        self.new_entry = true;
    }

    fn push_history(&mut self, line: String) {
        self.history.push(line);
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        }
    }

    fn to_rad(&self, x: f64) -> f64 {
        match self.angle {
            AngleMode::Deg => x * PI / 180.0,
            AngleMode::Rad => x,
        }
    }

    fn from_rad(&self, x: f64) -> f64 {
        match self.angle {
            AngleMode::Deg => x * 180.0 / PI,
            AngleMode::Rad => x,
        }
    }

    pub fn input_digit(&mut self, d: u8) {
        if self.error.is_some() {
            return;
        }
        if self.new_entry {
            self.entry.clear();
            self.new_entry = false;
        }
        let ch = (b'0' + d) as char;
        match self.entry.as_str() {
            "" | "0" => {
                self.entry.clear();
                self.entry.push(ch);
            }
            "-0" => {
                self.entry = String::from("-");
                self.entry.push(ch);
            }
            _ => {
                if self.entry.len() < MAX_ENTRY_LEN {
                    self.entry.push(ch);
                }
            }
        }
    }

    pub fn input_dot(&mut self) {
        if self.error.is_some() {
            return;
        }
        if self.new_entry {
            self.entry = "0.".to_string();
            self.new_entry = false;
            return;
        }
        if !self.entry.contains('.') && self.entry.len() < MAX_ENTRY_LEN {
            if self.entry.is_empty() || self.entry == "-" {
                self.entry.push('0');
            }
            self.entry.push('.');
        }
    }

    pub fn backspace(&mut self) {
        if self.error.is_some() {
            self.clear_all();
            return;
        }
        if self.new_entry {
            self.entry = "0".to_string();
            self.new_entry = false;
            return;
        }
        self.entry.pop();
        if self.entry.is_empty() || self.entry == "-" {
            self.entry = "0".to_string();
        }
    }

    pub fn negate(&mut self) {
        if self.error.is_some() {
            return;
        }
        if self.entry.starts_with('-') {
            self.entry.remove(0);
        } else if self.entry != "0" {
            self.entry.insert(0, '-');
        }
    }

    pub fn clear_entry(&mut self) {
        self.entry = "0".to_string();
        self.new_entry = false;
        self.error = None;
    }

    pub fn clear_all(&mut self) {
        self.entry = "0".to_string();
        self.acc = 0.0;
        self.pending = None;
        self.last_binary = None;
        self.new_entry = true;
        self.error = None;
    }

    pub fn apply_op(&mut self, op: Op) {
        if self.error.is_some() {
            return;
        }
        let cur = self.value();
        if self.pending.is_some() && !self.new_entry {
            match self.pending.unwrap().apply(self.acc, cur) {
                Ok(v) => self.acc = v,
                Err(e) => {
                    self.error = Some(e);
                    return;
                }
            }
        } else {
            self.acc = cur;
        }
        self.entry = format_num(self.acc);
        self.pending = Some(op);
        self.new_entry = true;
    }

    pub fn equals(&mut self) {
        if self.error.is_some() {
            return;
        }
        if let Some(op) = self.pending {
            let lhs = self.acc;
            let rhs = self.value();
            match op.apply(lhs, rhs) {
                Ok(v) => {
                    self.push_history(format!(
                        "{} {} {} = {}",
                        format_num(lhs),
                        op.symbol(),
                        format_num(rhs),
                        format_num(v)
                    ));
                    self.acc = v;
                    self.last_binary = Some((op, rhs));
                    self.pending = None;
                    self.set_result(v);
                }
                Err(e) => self.error = Some(e),
            }
        } else if let Some((op, rhs)) = self.last_binary {
            let lhs = self.value();
            match op.apply(lhs, rhs) {
                Ok(v) => {
                    self.push_history(format!(
                        "{} {} {} = {}",
                        format_num(lhs),
                        op.symbol(),
                        format_num(rhs),
                        format_num(v)
                    ));
                    self.acc = v;
                    self.set_result(v);
                }
                Err(e) => self.error = Some(e),
            }
        } else {
            self.set_result(self.value());
        }
    }

    pub fn apply_unary(&mut self, u: Unary) {
        if self.error.is_some() {
            return;
        }
        let x = self.value();
        let res: Result<f64, CalcError> = match u {
            Unary::Sin => finite(libm::sin(self.to_rad(x))),
            Unary::Cos => finite(libm::cos(self.to_rad(x))),
            Unary::Tan => finite(libm::tan(self.to_rad(x))),
            Unary::Asin => {
                if !(-1.0..=1.0).contains(&x) {
                    Err(CalcError::Domain)
                } else {
                    Ok(self.from_rad(libm::asin(x)))
                }
            }
            Unary::Acos => {
                if !(-1.0..=1.0).contains(&x) {
                    Err(CalcError::Domain)
                } else {
                    Ok(self.from_rad(libm::acos(x)))
                }
            }
            Unary::Atan => Ok(self.from_rad(libm::atan(x))),
            Unary::Ln => {
                if x <= 0.0 {
                    Err(CalcError::Domain)
                } else {
                    finite(libm::log(x))
                }
            }
            Unary::Log10 => {
                if x <= 0.0 {
                    Err(CalcError::Domain)
                } else {
                    finite(libm::log10(x))
                }
            }
            Unary::Exp => finite(libm::exp(x)),
            Unary::Exp10 => finite(libm::pow(10.0, x)),
            Unary::Sqr => finite(x * x),
            Unary::Sqrt => {
                if x < 0.0 {
                    Err(CalcError::Domain)
                } else {
                    finite(libm::sqrt(x))
                }
            }
            Unary::Cbrt => finite(libm::cbrt(x)),
            Unary::Recip => {
                if x == 0.0 {
                    Err(CalcError::DivZero)
                } else {
                    finite(1.0 / x)
                }
            }
            Unary::Fact => factorial(x),
        };
        match res {
            Ok(v) => {
                self.push_history(format!("{}({}) = {}", u.name(), format_num(x), format_num(v)));
                self.set_result(v);
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn input_constant(&mut self, which: Constant) {
        if self.error.is_some() {
            return;
        }
        let v = match which {
            Constant::Pi => PI,
            Constant::E => E,
        };
        self.entry = format_num(v);
        self.new_entry = false;
    }

    pub fn mem_store(&mut self) {
        self.memory = self.value();
        self.mem_set = true;
    }

    pub fn mem_recall(&mut self) {
        self.entry = format_num(self.memory);
        self.new_entry = false;
    }

    pub fn mem_add(&mut self) {
        self.memory += self.value();
        self.mem_set = true;
    }

    pub fn mem_sub(&mut self) {
        self.memory -= self.value();
        self.mem_set = true;
    }

    pub fn mem_clear(&mut self) {
        self.memory = 0.0;
        self.mem_set = false;
    }

    pub fn toggle_angle(&mut self) {
        self.angle = match self.angle {
            AngleMode::Deg => AngleMode::Rad,
            AngleMode::Rad => AngleMode::Deg,
        };
    }
}

/// \brief Mathematical constants offered in the menu.
#[derive(Clone, Copy)]
pub enum Constant {
    Pi,
    E,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        libm::fabs(a - b) < 1e-9
    }

    #[test]
    fn simple_add() {
        let mut c = Calc::new();
        c.input_digit(2);
        c.apply_op(Op::Add);
        c.input_digit(3);
        c.equals();
        assert_eq!(c.display(), "5");
    }

    #[test]
    fn chained_left_to_right() {
        let mut c = Calc::new();
        c.input_digit(2);
        c.apply_op(Op::Add);
        c.input_digit(3);
        c.apply_op(Op::Mul);
        c.input_digit(4);
        c.equals();
        assert_eq!(c.display(), "20");
    }

    #[test]
    fn divide_by_zero() {
        let mut c = Calc::new();
        c.input_digit(1);
        c.apply_op(Op::Div);
        c.input_digit(0);
        c.equals();
        assert_eq!(c.error(), Some(CalcError::DivZero));
    }

    #[test]
    fn sqrt_negative_is_domain_error() {
        let mut c = Calc::new();
        c.input_digit(1);
        c.negate();
        c.apply_unary(Unary::Sqrt);
        assert_eq!(c.error(), Some(CalcError::Domain));
    }

    #[test]
    fn ln_zero_is_domain_error() {
        let mut c = Calc::new();
        c.apply_unary(Unary::Ln);
        assert_eq!(c.error(), Some(CalcError::Domain));
    }

    #[test]
    fn sin_30_degrees_is_half() {
        let mut c = Calc::new();
        c.input_digit(3);
        c.input_digit(0);
        c.apply_unary(Unary::Sin);
        assert!(approx(c.value(), 0.5));
    }

    #[test]
    fn float_noise_is_hidden() {
        assert_eq!(format_num(0.1 + 0.2), "0.3");
    }

    #[test]
    fn large_value_is_scientific() {
        assert!(format_num(1e20).contains('e'));
    }

    #[test]
    fn decimal_and_backspace() {
        let mut c = Calc::new();
        c.input_digit(1);
        c.input_dot();
        c.input_digit(5);
        assert_eq!(c.display(), "1.5");
        c.backspace();
        assert_eq!(c.display(), "1.");
        c.backspace();
        assert_eq!(c.display(), "1");
    }

    #[test]
    fn memory_roundtrip() {
        let mut c = Calc::new();
        c.input_digit(7);
        c.mem_store();
        c.clear_all();
        c.mem_recall();
        assert_eq!(c.display(), "7");
    }

    #[test]
    fn equals_records_history() {
        let mut c = Calc::new();
        c.input_digit(2);
        c.apply_op(Op::Add);
        c.input_digit(3);
        c.equals();
        assert_eq!(c.history.len(), 1);
        assert_eq!(c.history[0], "2 + 3 = 5");
    }

    #[test]
    fn factorial_five() {
        let mut c = Calc::new();
        c.input_digit(5);
        c.apply_unary(Unary::Fact);
        assert_eq!(c.display(), "120");
    }
}
