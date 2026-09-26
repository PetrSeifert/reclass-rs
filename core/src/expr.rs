//! Address expressions, e.g. `[<game.exe> + 0x5A1230] + 0x10` or `[$GWorld]`.
//!
//! Grammar (same as the egui app's root address field):
//! ```text
//! expr   := factor (('+' | '-') factor)*
//! factor := '(' expr ')' | '[' expr ']' | '<' module '>' | '$' ident | number
//! number := '0x' hex+ | digit+
//! ```
//! `[x]` reads a u64 at address `x`. Arithmetic wraps.

/// Everything an expression can refer to.
pub trait ExprContext {
    fn module_base(&self, name: &str) -> Option<u64>;
    fn signature(&self, name: &str) -> Result<u64, String>;
    fn read_u64(&self, address: u64) -> Option<u64>;
}

pub fn eval(input: &str, ctx: &dyn ExprContext) -> Result<u64, String> {
    let mut p = Parser {
        s: input.as_bytes(),
        i: 0,
        ctx,
    };
    let v = p.expr()?;
    p.ws();
    match p.peek() {
        None => Ok(v),
        Some(c) => Err(p.err(&format!("unexpected '{}'", c as char))),
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    ctx: &'a dyn ExprContext,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn err(&self, msg: &str) -> String {
        format!("{msg} at {}", self.i)
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(self.err(&format!("expected '{}'", c as char)))
        }
    }

    fn expr(&mut self) -> Result<u64, String> {
        let mut acc = self.factor()?;
        loop {
            if self.eat(b'+') {
                acc = acc.wrapping_add(self.factor()?);
            } else if self.eat(b'-') {
                acc = acc.wrapping_sub(self.factor()?);
            } else {
                return Ok(acc);
            }
        }
    }

    fn factor(&mut self) -> Result<u64, String> {
        self.ws();
        if self.eat(b'(') {
            let v = self.expr()?;
            self.expect(b')')?;
            return Ok(v);
        }
        if self.eat(b'[') {
            let addr = self.expr()?;
            self.expect(b']')?;
            return self
                .ctx
                .read_u64(addr)
                .ok_or_else(|| format!("cannot read 0x{addr:X}"));
        }
        if self.eat(b'<') {
            let start = self.i;
            while self.peek().is_some_and(|b| b != b'>') {
                self.i += 1;
            }
            let name = std::str::from_utf8(&self.s[start..self.i])
                .unwrap_or("")
                .trim()
                .to_string();
            self.expect(b'>')?;
            return self
                .ctx
                .module_base(&name)
                .ok_or_else(|| format!("unknown module {name}"));
        }
        if self.eat(b'$') {
            let start = self.i;
            while self
                .peek()
                .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                self.i += 1;
            }
            if start == self.i {
                return Err(self.err("expected signature name"));
            }
            let name = std::str::from_utf8(&self.s[start..self.i]).unwrap();
            return self
                .ctx
                .signature(name)
                .map_err(|e| format!("${name}: {e}"));
        }
        self.number()
    }

    fn number(&mut self) -> Result<u64, String> {
        let rest = &self.s[self.i..];
        let (digits, radix, skip) = if rest.len() > 1 && rest[0] == b'0' && (rest[1] | 0x20) == b'x'
        {
            let n = rest[2..]
                .iter()
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            (&rest[2..2 + n], 16, 2)
        } else {
            let n = rest.iter().take_while(|b| b.is_ascii_digit()).count();
            (&rest[..n], 10, 0)
        };
        if digits.is_empty() {
            return Err(match self.peek() {
                None => self.err("unexpected end"),
                Some(c) => self.err(&format!("unexpected '{}'", c as char)),
            });
        }
        let text = std::str::from_utf8(digits).unwrap();
        let v = u64::from_str_radix(text, radix).map_err(|e| self.err(&e.to_string()))?;
        self.i += skip + digits.len();
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ctx;
    impl ExprContext for Ctx {
        fn module_base(&self, name: &str) -> Option<u64> {
            name.eq_ignore_ascii_case("game.exe").then_some(0x1000)
        }
        fn signature(&self, name: &str) -> Result<u64, String> {
            match name {
                "Good" => Ok(0x2000),
                _ => Err("pattern not found".into()),
            }
        }
        fn read_u64(&self, address: u64) -> Option<u64> {
            (address == 0x2000).then_some(0xABCD)
        }
    }

    #[test]
    fn evaluates_arithmetic_modules_signatures_and_derefs() {
        assert_eq!(eval("0x10 + 5 - 1", &Ctx), Ok(0x14));
        assert_eq!(eval(" <Game.exe> + (0x20 - 0x10) ", &Ctx), Ok(0x1010));
        assert_eq!(eval("[$Good] + 1", &Ctx), Ok(0xABCE));
        assert_eq!(eval("[<game.exe> + 0x1000]", &Ctx), Ok(0xABCD));
    }

    #[test]
    fn reports_errors() {
        assert_eq!(eval("[1", &Ctx), Err("expected ']' at 2".into()));
        assert_eq!(eval("$Bad", &Ctx), Err("$Bad: pattern not found".into()));
        assert_eq!(eval("[0x10]", &Ctx), Err("cannot read 0x10".into()));
        assert_eq!(
            eval("<nope.dll>", &Ctx),
            Err("unknown module nope.dll".into())
        );
        assert_eq!(eval("1 +", &Ctx), Err("unexpected end at 3".into()));
        assert_eq!(eval("1 2", &Ctx), Err("unexpected '2' at 2".into()));
    }
}
