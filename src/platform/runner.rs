//! Costura de comandos (ADR-029): tudo que o backend de DNS pede ao sistema passa por aqui.
//!
//! Produção usa `RealRunner` (o mesmo `std::process::Command` de antes, com os mesmos
//! argumentos, na mesma ordem e com o mesmo tratamento de stdin/stdout/stderr). Os testes
//! usam `fake::FakeRunner`, que grava o argv e simula o estado do DNS.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// O que o filho recebe no stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum In {
    /// Herda o stdin do processo (o padrão de `Command::status`).
    Inherit,
    /// `/dev/null` (o padrão de `Command::output`).
    Null,
    /// Bytes enviados por pipe, que é fechado em seguida.
    Data(Vec<u8>),
}

/// Destino do stdout/stderr do filho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    Inherit,
    Null,
    Capture,
}

/// Um comando a executar. `program` e `args` são o argv exato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<OsString>,
    pub stdin: In,
    pub stdout: Out,
    pub stderr: Out,
    /// Windows: `CREATE_NO_WINDOW` (nenhum console pisca).
    pub no_window: bool,
}

impl Cmd {
    /// Equivale a `Command::new(p).args(a).status()`: tudo herdado.
    pub fn status(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|a| (*a).into()).collect(),
            stdin: In::Inherit,
            stdout: Out::Inherit,
            stderr: Out::Inherit,
            no_window: false,
        }
    }

    /// Como `status`, com argumentos que podem não ser UTF-8 (caminhos).
    pub fn status_os(program: &str, args: Vec<OsString>) -> Self {
        Self {
            args,
            ..Self::status(program, &[])
        }
    }

    /// Equivale a `Command::new(p).args(a).output()`: captura stdout e stderr, stdin nulo.
    pub fn output(program: &str, args: &[&str]) -> Self {
        Self {
            stdin: In::Null,
            stdout: Out::Capture,
            stderr: Out::Capture,
            ..Self::status(program, args)
        }
    }

    pub fn stdin(mut self, i: In) -> Self {
        self.stdin = i;
        self
    }

    pub fn stdout(mut self, o: Out) -> Self {
        self.stdout = o;
        self
    }

    pub fn stderr(mut self, o: Out) -> Self {
        self.stderr = o;
        self
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn no_window(mut self) -> Self {
        self.no_window = true;
        self
    }

    /// Prefixa o comando com a elevação (`sudo`, `pkexec`...). Vazio = já é root.
    pub fn prefixed(mut self, prefix: &[&str]) -> Self {
        if let Some((tool, rest)) = prefix.split_first() {
            let mut args: Vec<OsString> = rest.iter().map(|a| (*a).into()).collect();
            args.push(std::mem::replace(&mut self.program, (*tool).into()).into());
            args.append(&mut self.args);
            self.args = args;
        }
        self
    }

    /// `[programa, args...]`, para comparar em testes.
    #[cfg(test)]
    pub fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().map(|a| a.to_string_lossy().into_owned()))
            .collect()
    }
}

/// Resultado de um comando (código de saída e o que foi capturado).
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// `None` = morto por sinal.
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub stderr: Vec<u8>,
}

impl Outcome {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// Tudo o que o backend precisa do sistema: comandos e as poucas leituras que mudam o argv.
pub trait CommandRunner {
    fn run(&self, cmd: &Cmd) -> io::Result<Outcome>;

    /// Executável no `PATH`.
    fn command_exists(&self, name: &str) -> bool {
        crate::sysutil::command_exists(name)
    }

    /// Caminho existe (arquivo ou diretório).
    fn path_exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }

    /// Conteúdo de um arquivo de texto; `None` se não existe ou não lê.
    fn read_file(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    /// O processo é root? (Unix)
    #[cfg(unix)]
    fn is_root(&self) -> bool {
        crate::sysutil::is_root()
    }

    /// Prefixo de elevação para comandos privilegiados.
    #[cfg(unix)]
    fn privilege_prefix(&self) -> Vec<&'static str> {
        crate::sysutil::current_privilege_prefix()
    }
}

/// Executa de verdade, com `std::process::Command`.
pub struct RealRunner;

impl CommandRunner for RealRunner {
    fn run(&self, cmd: &Cmd) -> io::Result<Outcome> {
        let mut c = Command::new(&cmd.program);
        c.args(&cmd.args);
        #[cfg(windows)]
        if cmd.no_window {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        c.stdin(match cmd.stdin {
            In::Inherit => Stdio::inherit(),
            In::Null => Stdio::null(),
            In::Data(_) => Stdio::piped(),
        });
        let io_of = |o: Out| match o {
            Out::Inherit => Stdio::inherit(),
            Out::Null => Stdio::null(),
            Out::Capture => Stdio::piped(),
        };
        c.stdout(io_of(cmd.stdout)).stderr(io_of(cmd.stderr));

        let captures = cmd.stdout == Out::Capture || cmd.stderr == Out::Capture;
        if !captures && !matches!(cmd.stdin, In::Data(_)) {
            let st = c.status()?;
            return Ok(Outcome {
                code: st.code(),
                ..Outcome::default()
            });
        }
        let mut child = c.spawn()?;
        if let In::Data(data) = &cmd.stdin {
            // O pipe fecha ao fim do bloco: o filho vê EOF.
            child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("stdin do filho indisponível"))?
                .write_all(data)?;
        }
        let out = child.wait_with_output()?;
        Ok(Outcome {
            code: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixed_poe_a_elevacao_antes_do_programa() {
        let c = Cmd::status("nmcli", &["connection", "up"]).prefixed(&["sudo", "-n"]);
        assert_eq!(c.argv(), ["sudo", "-n", "nmcli", "connection", "up"]);
        let c = Cmd::status("tee", &["/etc/resolv.conf"]).prefixed(&[]);
        assert_eq!(c.argv(), ["tee", "/etc/resolv.conf"]);
    }

    #[test]
    fn construtores_espelham_status_e_output() {
        let s = Cmd::status("x", &[]);
        assert_eq!(
            (s.stdin, s.stdout, s.stderr),
            (In::Inherit, Out::Inherit, Out::Inherit)
        );
        let o = Cmd::output("x", &[]);
        assert_eq!(
            (o.stdin, o.stdout, o.stderr),
            (In::Null, Out::Capture, Out::Capture)
        );
    }

    #[cfg(unix)]
    #[test]
    fn real_runner_captura_stdout_codigo_e_stdin() {
        let r = RealRunner;
        let o = r
            .run(&Cmd::output("sh", &["-c", "echo oi; exit 3"]))
            .unwrap();
        assert_eq!(o.code, Some(3));
        assert_eq!(o.stdout_text(), "oi\n");
        assert!(!o.success());
        let o = r
            .run(&Cmd::output("cat", &[]).stdin(In::Data(b"dados".to_vec())))
            .unwrap();
        assert!(o.success());
        assert_eq!(o.stdout_text(), "dados");
        let o = r
            .run(&Cmd::status("true", &[]).stdout(Out::Null).stderr(Out::Null))
            .unwrap();
        assert!(o.success());
        assert!(r.run(&Cmd::status("guialar-nao-existe-xyz", &[])).is_err());
    }
}
