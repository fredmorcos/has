#![warn(clippy::all)]

use io::{Bytes, Read};
use std::collections::HashMap as Map;
use std::{fmt, io};

use crate::lex::Txt;
use crate::lex::{self, Comp, Dest, Jump, Lex, Tok};
use crate::pos::Pos;

#[derive(Debug, PartialEq, Eq)]
pub enum Stmt {
  Addr(Pos, u16),
  UnresolvedAddr(Pos, Txt),
  Assign(Pos, Dest, Pos, Comp),
  Branch(Pos, Comp, Pos, Jump),
  Inst(Pos, Dest, Pos, Comp, Pos, Jump),
}

fn is_predefined_symbol(s: &[u8]) -> Option<u16> {
  match s {
    b"SP" => Some(0),
    b"LCL" => Some(1),
    b"ARG" => Some(2),
    b"THIS" => Some(3),
    b"THAT" => Some(4),
    b"R0" => Some(0),
    b"R1" => Some(1),
    b"R2" => Some(2),
    b"R3" => Some(3),
    b"R4" => Some(4),
    b"R5" => Some(5),
    b"R6" => Some(6),
    b"R7" => Some(7),
    b"R8" => Some(8),
    b"R9" => Some(9),
    b"R10" => Some(10),
    b"R11" => Some(11),
    b"R12" => Some(12),
    b"R13" => Some(13),
    b"R14" => Some(14),
    b"R15" => Some(15),
    b"SCREEN" => Some(16384),
    b"KBD" => Some(24576),
    _ => None,
  }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct SymInfo {
  pub pos: Pos,
  pub addr: u16,
}

impl SymInfo {
  pub fn new(pos: Pos, addr: u16) -> Self {
    Self { pos, addr }
  }
}

pub struct Parse<'s, R: Read> {
  lex: Lex<R>,
  st: &'s mut Map<Txt, SymInfo>,
  la: Option<Tok>,
  idx: u16,
}

impl<'s, R: Read> Parse<'s, R> {
  pub fn new(bytes: Bytes<R>, st: &'s mut Map<Txt, SymInfo>) -> Self {
    Self {
      lex: Lex::from(bytes),
      st,
      la: Option::default(),
      idx: 0,
    }
  }
}

#[derive(PartialEq, Eq)]
pub enum Err {
  Lex(lex::Err),
  Label(Pos, Txt, SymInfo),
  Msg(Pos, &'static str),
  Dest(Pos, Dest),
  Comp(Pos, Comp),
  Jump(Pos, Jump),
  Semi(Pos),
}

impl fmt::Display for Err {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      Err::Lex(e) => write!(f, "Lexing error: {}", e),
      Err::Label(pos, name, orig) => write!(
        f,
        "Label {:?} at {} already defined at {} with address {}",
        name, pos, orig.pos, orig.addr,
      ),
      Err::Msg(pos, msg) => write!(f, "expecting {} after {}", msg, pos),
      Err::Dest(pos, dest) => write!(
        f,
        "destination {} at {} must be followed by a computation",
        dest, pos
      ),
      Err::Comp(pos, comp) => write!(
        f,
        "computation {} at {} must be followed by a jump",
        comp, pos
      ),
      Err::Jump(pos, jump) => write!(
        f,
        "jump {} at {} must be preceded by a computation",
        jump, pos
      ),
      Err::Semi(pos) => write!(f, "freestanding semicolon at {}", pos),
    }
  }
}

impl fmt::Debug for Err {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    (self as &dyn fmt::Display).fmt(f)
  }
}

impl<'s, R: Read> Iterator for Parse<'s, R> {
  type Item = Result<Stmt, Err>;

  fn next(&mut self) -> Option<Self::Item> {
    macro_rules! next {
      ($b:block) => {
        match self.lex.next() {
          Some(Ok(t)) => t,
          Some(Err(e)) => return Some(Err(Err::Lex(e))),
          None => $b,
        };
      };
    }

    let t1 = if let Some(la) = self.la.take() {
      la
    } else {
      next!({ return None })
    };

    match t1 {
      Tok::NumAddr(pos, addr) => {
        self.idx += 1;
        Some(Ok(Stmt::Addr(pos, addr)))
      }
      Tok::NameAddr(pos, name) => {
        self.idx += 1;
        if let Some(addr) = is_predefined_symbol(&name) {
          Some(Ok(Stmt::Addr(pos, addr)))
        } else if let Some(info) = self.st.get(&name) {
          Some(Ok(Stmt::Addr(pos, info.addr)))
        } else {
          Some(Ok(Stmt::UnresolvedAddr(pos, name)))
        }
      }
      Tok::Label(pos, label) => {
        if let Some(old) = self.st.insert(label.clone(), SymInfo::new(pos, self.idx)) {
          Some(Err(Err::Label(pos, label, old)))
        } else {
          self.next()
        }
      }
      Tok::Dest(dest_pos, dest) => {
        self.idx += 1;
        match next!({ return Some(Err(Err::Msg(dest_pos, "a computation"))) }) {
          Tok::Comp(comp_pos, comp) => {
            match next!({ return Some(Ok(Stmt::Assign(dest_pos, dest, comp_pos, comp))) })
            {
              Tok::Semi(semi_pos) => {
                match next!({ return Some(Err(Err::Msg(semi_pos, "a jump"))) }) {
                  Tok::Jump(jump_pos, jump) => Some(Ok(Stmt::Inst(
                    dest_pos, dest, comp_pos, comp, jump_pos, jump,
                  ))),
                  Tok::NumAddr(_, _)
                  | Tok::NameAddr(_, _)
                  | Tok::Label(_, _)
                  | Tok::Semi(_)
                  | Tok::Dest(_, _)
                  | Tok::Comp(_, _) => Some(Err(Err::Msg(semi_pos, "a jump"))),
                }
              }
              la => {
                self.la = Some(la);
                Some(Ok(Stmt::Assign(dest_pos, dest, comp_pos, comp)))
              }
            }
          }
          Tok::NumAddr(_, _)
          | Tok::NameAddr(_, _)
          | Tok::Label(_, _)
          | Tok::Semi(_)
          | Tok::Dest(_, _)
          | Tok::Jump(_, _) => Some(Err(Err::Dest(dest_pos, dest))),
        }
      }
      Tok::Comp(comp_pos, comp) => {
        self.idx += 1;
        match next!({ return Some(Err(Err::Comp(comp_pos, comp))) }) {
          Tok::Semi(semi_pos) => {
            match next!({ return Some(Err(Err::Msg(semi_pos, "a jump"))) }) {
              Tok::Jump(jump_pos, jump) => {
                Some(Ok(Stmt::Branch(comp_pos, comp, jump_pos, jump)))
              }
              Tok::NumAddr(_, _)
              | Tok::NameAddr(_, _)
              | Tok::Label(_, _)
              | Tok::Semi(_)
              | Tok::Dest(_, _)
              | Tok::Comp(_, _) => Some(Err(Err::Msg(semi_pos, "a jump"))),
            }
          }
          Tok::Jump(_, _)
          | Tok::NumAddr(_, _)
          | Tok::NameAddr(_, _)
          | Tok::Label(_, _)
          | Tok::Dest(_, _)
          | Tok::Comp(_, _) => Some(Err(Err::Comp(comp_pos, comp))),
        }
      }
      Tok::Jump(pos, jump) => Some(Err(Err::Jump(pos, jump))),
      Tok::Semi(pos) => Some(Err(Err::Semi(pos))),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap as Map;
  use std::io::{BufReader, Read};

  use crate::lex::Comp;
  use crate::lex::Dest;
  use crate::lex::Jump;
  use crate::lex::Txt;
  use crate::pos::Pos;

  use super::Parse;
  use super::Stmt;

  macro_rules! parse {
    ($f:expr, $st:expr) => {
      Parse::new(
        BufReader::new(&include_bytes!(concat!("../tests/data/", $f))[..]).bytes(),
        $st,
      )
    };
  }

  macro_rules! assert_next {
    ($parse:expr, $e:expr) => {
      assert_eq!($parse.next(), Some(Ok($e)))
    };
  }

  #[test]
  fn empty() {
    let mut st = Map::new();
    let mut parse = parse!("empty", &mut st);
    assert_eq!(parse.next(), None);
  }

  #[test]
  fn spaces() {
    let mut st = Map::new();
    let mut parse = parse!("spaces", &mut st);
    assert_eq!(parse.next(), None);
  }

  #[test]
  fn comments() {
    let mut st = Map::new();
    let mut parse = parse!("comments", &mut st);
    assert_eq!(parse.next(), None);
  }

  #[test]
  fn num_address() {
    let mut st = Map::new();
    let mut parse = parse!("num_address", &mut st);
    assert_next!(parse, Stmt::Addr(Pos::new(3, 5), 8192));
    assert_next!(parse, Stmt::Addr(Pos::new(5, 1), 123));
    assert_next!(parse, Stmt::Addr(Pos::new(9, 5), 556));
    assert_eq!(parse.next(), None);
  }

  #[test]
  fn name_address() {
    let mut st = Map::new();
    let mut parse = parse!("name_address", &mut st);
    assert_next!(
      parse,
      Stmt::UnresolvedAddr(Pos::new(3, 5), Txt::from(&b"FOO"[..]))
    );
    assert_next!(
      parse,
      Stmt::UnresolvedAddr(Pos::new(5, 1), Txt::from(&b"BAR"[..]))
    );
    assert_next!(parse, Stmt::Addr(Pos::new(9, 5), 2));
    assert_eq!(parse.next(), None);
  }

  #[test]
  fn labels() {
    let mut st = Map::new();
    let mut parse = parse!("labels", &mut st);
    assert_next!(
      parse,
      Stmt::UnresolvedAddr(Pos::new(3, 5), Txt::from(&b"FOO"[..]))
    );
    assert_next!(parse, Stmt::Addr(Pos::new(9, 5), 1));
    assert_next!(
      parse,
      Stmt::UnresolvedAddr(Pos::new(11, 1), Txt::from(&b"BAR"[..]))
    );
    assert_next!(
      parse,
      Stmt::UnresolvedAddr(Pos::new(15, 1), Txt::from(&b"LAB0"[..]))
    );
    assert_eq!(parse.next(), None);
  }

  #[test]
  fn assignments() {
    let mut st = Map::new();
    let mut parse = parse!("assignments", &mut st);
    assert_next!(
      parse,
      Stmt::Assign(Pos::new(1, 1), Dest::A, Pos::new(1, 3), Comp::MMinus1)
    );
    assert_next!(
      parse,
      Stmt::Assign(Pos::new(2, 1), Dest::AM, Pos::new(2, 4), Comp::DOrA,)
    );
    assert_next!(
      parse,
      Stmt::Assign(Pos::new(3, 1), Dest::AMD, Pos::new(3, 5), Comp::APlus1,)
    );
    assert_eq!(parse.next(), None);
  }

  #[test]
  fn branches() {
    let mut st = Map::new();
    let mut parse = parse!("branches", &mut st);
    assert_next!(
      parse,
      Stmt::Branch(Pos::new(1, 1), Comp::MMinus1, Pos::new(1, 5), Jump::JEQ)
    );
    assert_next!(
      parse,
      Stmt::Branch(Pos::new(2, 1), Comp::DOrA, Pos::new(2, 5), Jump::JNE)
    );
    assert_next!(
      parse,
      Stmt::Branch(Pos::new(3, 1), Comp::APlus1, Pos::new(3, 5), Jump::JMP)
    );
    assert_eq!(parse.next(), None);
  }

  #[test]
  fn instructions() {
    let mut st = Map::new();
    let mut parse = parse!("instructions", &mut st);
    assert_next!(
      parse,
      Stmt::Inst(
        Pos::new(1, 1),
        Dest::A,
        Pos::new(1, 3),
        Comp::MMinus1,
        Pos::new(1, 7),
        Jump::JEQ
      )
    );
    assert_next!(
      parse,
      Stmt::Inst(
        Pos::new(2, 1),
        Dest::AM,
        Pos::new(2, 4),
        Comp::DOrA,
        Pos::new(2, 8),
        Jump::JNE
      )
    );
    assert_next!(
      parse,
      Stmt::Inst(
        Pos::new(3, 1),
        Dest::AMD,
        Pos::new(3, 5),
        Comp::APlus1,
        Pos::new(3, 9),
        Jump::JMP
      )
    );
    assert_eq!(parse.next(), None);
  }
}
