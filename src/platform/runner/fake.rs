//! `FakeRunner`: grava o argv de cada comando e simula o estado de DNS que o
//! NetworkManager, o `tee`/`rm`/`mkdir` e o `resolvconf` alterariam. Nada toca o sistema.

#![cfg_attr(not(unix), allow(dead_code))]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io;

use super::{Cmd, CommandRunner, In, Outcome};

/// Um comando executado: argv completo (com o prefixo de elevação) e o stdin enviado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub argv: Vec<String>,
    pub stdin: Option<String>,
}

type Props = BTreeMap<String, String>;

/// Estado observável do DNS (o que `apply ∘ revert` deve devolver igual).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsState {
    /// uuid -> propriedades `ipv4.dns`, `ipv4.ignore-auto-dns`, `ipv6.dns`, `ipv6.ignore-auto-dns`.
    pub nm: BTreeMap<String, Props>,
    /// Arquivos gravados (inclui `/etc/resolv.conf`, o drop-in do resolved e o `resolvconf`).
    pub files: BTreeMap<String, String>,
}

#[derive(Default)]
struct State {
    calls: Vec<Call>,
    files: BTreeMap<String, String>,
    dirs: BTreeSet<String>,
    paths: BTreeSet<String>,
    commands: BTreeSet<String>,
    nm: BTreeMap<String, Props>,
    nm_type: BTreeMap<String, String>,
    deny: Vec<(String, i32)>,
    fail_argv: Vec<Vec<String>>,
    prefix: Vec<&'static str>,
    root: bool,
}

pub struct FakeRunner {
    st: RefCell<State>,
}

fn props(dns4: &str, ign4: bool) -> Props {
    let mut p = Props::new();
    p.insert("ipv4.dns".into(), dns4.into());
    p.insert("ipv4.ignore-auto-dns".into(), yn(ign4).into());
    p.insert("ipv6.dns".into(), String::new());
    p.insert("ipv6.ignore-auto-dns".into(), "no".into());
    p
}

fn yn(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// nmcli aceita lista por espaço ou vírgula; guardamos normalizado (espaço).
fn norm(v: &str) -> String {
    v.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

impl FakeRunner {
    /// Máquina "root" (sem prefixo de elevação), sem nada instalado.
    pub fn new() -> Self {
        let f = Self {
            st: RefCell::new(State::default()),
        };
        f.st.borrow_mut().root = true;
        f
    }

    /// Conexão ativa do NetworkManager com DNS/ignore-auto iniciais.
    pub fn with_nm_conn(self, uuid: &str, kind: &str, dns4: &str, ign4: bool) -> Self {
        {
            let mut s = self.st.borrow_mut();
            s.nm.insert(uuid.into(), props(dns4, ign4));
            s.nm_type.insert(uuid.into(), kind.into());
        }
        self.with_path("/run/NetworkManager").with_command("nmcli")
    }

    /// IPv6 inicial da conexão (`with_nm_conn` deixa `ipv6.dns` vazio e `ignore-auto-dns=no`).
    pub fn with_nm_v6(self, uuid: &str, dns6: &str, ign6: bool) -> Self {
        {
            let mut s = self.st.borrow_mut();
            let p =
                s.nm.get_mut(uuid)
                    .expect("with_nm_conn antes de with_nm_v6");
            p.insert("ipv6.dns".into(), norm(dns6));
            p.insert("ipv6.ignore-auto-dns".into(), yn(ign6).into());
        }
        self
    }

    pub fn with_file(self, path: &str, content: &str) -> Self {
        self.st
            .borrow_mut()
            .files
            .insert(path.into(), content.into());
        self
    }

    pub fn with_dir(self, path: &str) -> Self {
        self.st.borrow_mut().dirs.insert(path.into());
        self
    }

    pub fn with_path(self, path: &str) -> Self {
        self.st.borrow_mut().paths.insert(path.into());
        self
    }

    pub fn with_command(self, name: &str) -> Self {
        self.st.borrow_mut().commands.insert(name.into());
        self
    }

    /// O processo não é root e usa este prefixo (ex.: `["sudo", "-n"]`).
    pub fn not_root(self, prefix: &[&'static str]) -> Self {
        {
            let mut s = self.st.borrow_mut();
            s.root = false;
            s.prefix = prefix.to_vec();
        }
        self
    }

    /// Muda de root para usuário comum (ou o contrário) no meio do teste.
    pub fn set_root(&self, root: bool) {
        self.st.borrow_mut().root = root;
    }

    /// Como `deny`, mas por referência (para trocar o comportamento no meio do teste).
    pub fn add_deny(&self, program: &str, code: i32) {
        self.st.borrow_mut().deny.push((program.into(), code));
    }

    /// O comando cujo argv começa com `prefix` não chega a executar (erro de E/S, como um
    /// `nmcli` que não sobe).
    pub fn add_fail_argv(&self, prefix: &[&str]) {
        self.st
            .borrow_mut()
            .fail_argv
            .push(prefix.iter().map(|p| p.to_string()).collect());
    }

    pub fn clear_deny(&self) {
        self.st.borrow_mut().deny.clear();
    }

    /// Todo comando cujo `argv[0]` for `program` sai com `code` sem alterar nada
    /// (ex.: `pkexec` com 126 = permissão negada).
    pub fn deny(self, program: &str, code: i32) -> Self {
        self.st.borrow_mut().deny.push((program.into(), code));
        self
    }

    pub fn calls(&self) -> Vec<Call> {
        self.st.borrow().calls.clone()
    }

    /// Só os argv, em ordem.
    pub fn argvs(&self) -> Vec<Vec<String>> {
        self.calls().into_iter().map(|c| c.argv).collect()
    }

    pub fn clear_calls(&self) {
        self.st.borrow_mut().calls.clear();
    }

    pub fn state(&self) -> DnsState {
        let s = self.st.borrow();
        DnsState {
            nm: s.nm.clone(),
            files: s.files.clone(),
        }
    }

    pub fn nm_prop(&self, uuid: &str, key: &str) -> String {
        self.st.borrow().nm[uuid][key].clone()
    }

    fn handle(&self, argv: &[String], stdin: Option<&str>) -> io::Result<Outcome> {
        let ok = |out: String| Outcome {
            code: Some(0),
            stdout: out.into_bytes(),
            stderr: Vec::new(),
        };
        let a: Vec<&str> = argv.iter().map(String::as_str).collect();
        let mut s = self.st.borrow_mut();
        match a.as_slice() {
            ["nmcli", "general", "status"] => Ok(ok("STATE running\n".into())),
            ["nmcli", "-t", "-f", "UUID,TYPE", "connection", "show", "--active"] => {
                let mut out = String::new();
                for u in s.nm.keys() {
                    out += &format!("{u}:{}\n", s.nm_type[u]);
                }
                Ok(ok(out))
            }
            ["nmcli", "-t", "-f", "ipv4.dns,ipv4.ignore-auto-dns", "connection", "show", u] => {
                let p = s.nm.get(*u).ok_or_else(|| io::Error::other("conexão?"))?;
                Ok(ok(format!(
                    "ipv4.dns:{}\nipv4.ignore-auto-dns:{}\n",
                    p["ipv4.dns"].replace(' ', ","),
                    p["ipv4.ignore-auto-dns"]
                )))
            }
            ["nmcli", "-t", "-f", "ipv6.dns,ipv6.ignore-auto-dns", "connection", "show", u] => {
                let p = s.nm.get(*u).ok_or_else(|| io::Error::other("conexão?"))?;
                Ok(ok(format!(
                    "ipv6.dns:{}\nipv6.ignore-auto-dns:{}\n",
                    p["ipv6.dns"].replace(' ', ","),
                    p["ipv6.ignore-auto-dns"]
                )))
            }
            ["nmcli", "connection", "modify", u, rest @ ..] => {
                let p =
                    s.nm.get_mut(*u)
                        .ok_or_else(|| io::Error::other("conexão?"))?;
                for kv in rest.chunks(2) {
                    let v = if kv[0].ends_with("dns") && !kv[0].contains("ignore") {
                        norm(kv[1])
                    } else {
                        kv[1].to_string()
                    };
                    p.insert(kv[0].into(), v);
                }
                Ok(ok(String::new()))
            }
            ["nmcli", "-t", "-f", "IP4.DNS,IP6.DNS", "device", "show"] => {
                // DNS em uso: os IPs fixos da conexão ou, sem eles, o do DHCP (10.0.2.3).
                let mut out = String::new();
                let (mut n4, mut n6) = (0, 0);
                for p in s.nm.values() {
                    let v4: Vec<&str> = p["ipv4.dns"].split_whitespace().collect();
                    for ip in if v4.is_empty() { vec!["10.0.2.3"] } else { v4 } {
                        n4 += 1;
                        out += &format!("IP4.DNS[{n4}]:{ip}\n");
                    }
                    for ip in p["ipv6.dns"].split_whitespace() {
                        n6 += 1;
                        out += &format!("IP6.DNS[{n6}]:{ip}\n");
                    }
                }
                Ok(ok(out))
            }
            ["nmcli", "connection", "up", "uuid", _] => Ok(ok(String::new())),
            ["sudo", ..] | ["pkexec", ..] => Ok(ok(String::new())),
            ["tee", path] => {
                s.files.insert((*path).into(), stdin.unwrap_or("").into());
                Ok(ok(String::new()))
            }
            ["mkdir", "-p", dir] => {
                s.dirs.insert((*dir).into());
                Ok(ok(String::new()))
            }
            ["rm", "-f", path] => {
                s.files.remove(*path);
                Ok(ok(String::new()))
            }
            ["systemctl", "restart", _] => Ok(ok(String::new())),
            ["resolvconf", "-a", name] => {
                s.files
                    .insert(format!("resolvconf:{name}"), stdin.unwrap_or("").into());
                Ok(ok(String::new()))
            }
            ["resolvconf", "-d", name] => {
                s.files.remove(&format!("resolvconf:{name}"));
                Ok(ok(String::new()))
            }
            other => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("comando inesperado no FakeRunner: {other:?}"),
            )),
        }
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, cmd: &Cmd) -> io::Result<Outcome> {
        let stdin = match &cmd.stdin {
            In::Data(d) => Some(String::from_utf8_lossy(d).into_owned()),
            _ => None,
        };
        let argv = cmd.argv();
        self.st.borrow_mut().calls.push(Call {
            argv: argv.clone(),
            stdin: stdin.clone(),
        });
        // Tira o prefixo de elevação (só se o argv começa mesmo com ele).
        let prefix: Vec<String> = self
            .st
            .borrow()
            .prefix
            .iter()
            .map(|p| p.to_string())
            .collect();
        let real = if !prefix.is_empty() && argv.len() > prefix.len() && argv.starts_with(&prefix) {
            &argv[prefix.len()..]
        } else {
            &argv[..]
        };
        // `deny` casa com o programa de verdade ou com a ferramenta de elevação.
        let denied = self
            .st
            .borrow()
            .deny
            .iter()
            .find(|(p, _)| *p == real[0] || *p == argv[0])
            .map(|(_, c)| *c);
        if self
            .st
            .borrow()
            .fail_argv
            .iter()
            .any(|p| real.starts_with(p))
        {
            return Err(io::Error::other("falha simulada pelo FakeRunner"));
        }
        if let Some(code) = denied {
            return Ok(Outcome {
                code: Some(code),
                ..Outcome::default()
            });
        }
        self.handle(real, stdin.as_deref())
    }

    fn command_exists(&self, name: &str) -> bool {
        self.st.borrow().commands.contains(name)
    }

    fn path_exists(&self, path: &str) -> bool {
        let s = self.st.borrow();
        s.files.contains_key(path) || s.dirs.contains(path) || s.paths.contains(path)
    }

    fn read_file(&self, path: &str) -> Option<String> {
        self.st.borrow().files.get(path).cloned()
    }

    #[cfg(unix)]
    fn is_root(&self) -> bool {
        self.st.borrow().root
    }

    #[cfg(unix)]
    fn privilege_prefix(&self) -> Vec<&'static str> {
        self.st.borrow().prefix.clone()
    }
}
