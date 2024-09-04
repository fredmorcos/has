#![warn(clippy::all)]

use clap::Parser;
use derive_more::Display;
use derive_more::From;
use has::hack;
use has::hack::dec;
use has::HackProg;
use has::HackProgErr;
use log::{debug, info, trace};
use std::fmt;
use std::fs::File;
use std::io;
use std::io::BufWriter;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

#[derive(From, Display)]
#[display(fmt = "Error: {}")]
enum Err {
  #[display(fmt = "IO error: {}", _0)]
  Io(io::Error),

  #[display(fmt = "Assembler error: {}", _0)]
  Asm(HackProgErr),

  #[display(fmt = "Disassembler error: {}", _0)]
  Dis(dec::Err),

  #[display(fmt = "Decoding error: {}", _0)]
  Decode(hack::CmdErr),
}

impl fmt::Debug for Err {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    (self as &dyn fmt::Display).fmt(f)
  }
}

/// The HACK Application Suite.
#[derive(Debug, clap::Parser)]
#[clap(author, version, about, long_about = None)]
struct Opt {
  /// Verbose output (can be specified multiple times).
  #[clap(short, long, action = clap::ArgAction::Count)]
  verbose: u8,

  /// HAS sub-command.
  #[clap(subcommand)]
  command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
  /// Assemble a HACK file.
  Asm {
    /// Output a bintext instead of binary file.
    #[clap(short, long)]
    bintext: bool,

    /// Output file (must not exist).
    #[clap(short, long, name = "OUT")]
    out: PathBuf,

    /// Hack assembly file to compile.
    #[clap(name = "FILE")]
    file: PathBuf,
  },

  /// Disassemble a HACK file.
  Dis {
    /// The input is a bintext instead of a binary file.
    #[clap(short, long)]
    bintext: bool,

    /// Output file (must not exist).
    #[clap(short, long, name = "OUT")]
    out: PathBuf,

    /// Hack file to disassemble.
    #[clap(name = "FILE")]
    file: PathBuf,
  },
}

impl Command {
  fn exec(self) -> Result<(), Err> {
    match self {
      Command::Asm { bintext, out, file } => exec_asm(bintext, out, file),
      Command::Dis { bintext, out, file } => exec_dis(bintext, out, file),
    }
  }
}

fn read_file(file: &Path) -> Result<Vec<u8>, Err> {
  let mut buf = Vec::with_capacity(1024);
  let bytes = File::open(file)?.read_to_end(&mut buf)?;
  info!("Read {} bytes from {}", bytes, file.display());
  Ok(buf)
}

fn ensure_available_outfile(out: &Path) -> Result<(), Err> {
  if out.exists() {
    return Err(Err::Io(io::Error::new(
      io::ErrorKind::AlreadyExists,
      format!("File {} already exists", out.display()),
    )));
  }

  Ok(())
}

fn create_outfile(out: &Path) -> Result<BufWriter<File>, Err> {
  let output = File::create(out)?;
  let writer = BufWriter::new(output);
  info!("Writing to file {}", out.display());
  Ok(writer)
}

fn exec_asm(text: bool, out: PathBuf, file: PathBuf) -> Result<(), Err> {
  ensure_available_outfile(&out)?;
  let buf = read_file(&file)?;

  info!("Parsing {}", file.display());
  let prog = HackProg::from_source(buf.as_slice())?;
  let mut writer = create_outfile(&out)?;

  if text {
    for inst in prog.to_bintext() {
      let inst = inst?;
      writer.write_all(&inst)?;
      writer.write_all(&[b'\n'])?;
    }
  } else {
    for inst in prog.to_bin() {
      let inst = inst?;
      writer.write_all(&inst)?;
    }
  }

  Ok(())
}

fn exec_dis(text: bool, out: PathBuf, file: PathBuf) -> Result<(), Err> {
  ensure_available_outfile(&out)?;
  let buf = read_file(&file)?;

  info!("Parsing {}", file.display());
  let prog = if text { HackProg::from_bintext(&buf)? } else { HackProg::from_bin(&buf)? };

  let mut writer = create_outfile(&out)?;

  for inst in prog.to_source() {
    writer.write_all(inst.as_bytes())?;
    writer.write_all(&[b'\n'])?;
  }

  Ok(())
}

fn main() -> Result<(), Err> {
  let opt = Opt::parse();

  let log_level = match opt.verbose {
    0 => log::LevelFilter::Warn,
    1 => log::LevelFilter::Info,
    2 => log::LevelFilter::Debug,
    _ => log::LevelFilter::Trace,
  };

  env_logger::Builder::new().filter_level(log_level).try_init().unwrap_or_else(|e| {
    eprintln!("Error initializing logger: {}", e);
  });

  info!("Informational output enabled.");
  debug!("Debug output enabled.");
  trace!("Tracing output enabled.");

  opt.command.exec()
}
